use crate::pretty_print::pretty_print_xml;
use xee_xpath::Itemable;
use xee_xpath::Query;
use xee_xpath::context::StaticContextBuilder;
use xee_xpath::error::Error;
use xee_xpath::error::SourceSpan;

#[derive(Debug, thiserror::Error)]
enum XpathEvalError {
    #[error("{0}")]
    XPath(#[from] xee_xpath::error::Error),
    #[error("{0}")]
    NamespaceDecl(String),
}

/// Pre-parsed XML document that can be reused across multiple XPath evaluations,
/// avoiding the cost of re-parsing the XML string each time.
pub struct XpathDocCache {
    documents: xee_xpath::Documents,
    doc_handle: xee_xpath::DocumentHandle,
}

// SAFETY: XpathDocCache is only ever accessed from the single thread that owns the
// UITree. The inner Rc<RefCell<…>> inside xee_xpath::Documents is never shared
// across threads — it is created on the owning thread and all access stays there.
unsafe impl Send for XpathDocCache {}

impl std::fmt::Debug for XpathDocCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XpathDocCache")
            .field("cached", &true)
            .finish()
    }
}

impl XpathDocCache {
    pub fn new(xml: &str) -> Option<Self> {
        let mut documents = xee_xpath::Documents::new();
        let doc_handle = documents.add_string_without_uri(xml).ok()?;
        Some(XpathDocCache {
            documents,
            doc_handle,
        })
    }
}

/// Evaluate an XPath expression against a pre-parsed XML document cache.
/// This skips the XML parsing step, reusing the already-parsed DOM.
pub fn eval_xpath_on_cache(expr: &str, cache: &mut XpathDocCache) -> XpathResult {
    let static_context_builder = match make_static_context_builder(None, &[]) {
        Ok(ctx) => ctx,
        Err(e) => {
            return XpathResult::new(
                false,
                Some(format!("Failed to build XPath context: {}", e)),
                0,
                vec![],
            );
        }
    };

    let queries = xee_xpath::Queries::new(static_context_builder);
    match execute_query(
        expr,
        &queries,
        &mut cache.documents,
        Some(cache.doc_handle),
    ) {
        Ok(res) => res,
        Err(e) => XpathResult::new(
            false,
            Some(format!("XPath query execution failed: {}", e)),
            0,
            vec![],
        ),
    }
}

#[derive(Debug, Clone)]
pub struct XpathQueryResult {
    item_xml: String,
    item_value: String,
}

impl XpathQueryResult {
    fn new(item_xml: String, item_value: String) -> Self {
        XpathQueryResult {
            item_xml,
            item_value,
        }
    }

    pub fn get_item_xml(&self) -> &str {
        &self.item_xml
    }

    pub fn get_item_value(&self) -> &str {
        &self.item_value
    }
}

impl Default for XpathQueryResult {
    fn default() -> Self {
        XpathQueryResult {
            item_xml: "".to_string(),
            item_value: "".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct XpathResult {
    success: bool,
    error_msg: Option<String>,
    result_count: usize,
    result: Vec<XpathQueryResult>,
}

impl XpathResult {
    fn new(
        success: bool,
        error_msg: Option<String>,
        result_count: usize,
        result: Vec<XpathQueryResult>,
    ) -> Self {
        XpathResult {
            success,
            error_msg,
            result_count,
            result,
        }
    }

    pub fn set_success(&mut self, success: bool) {
        self.success = success;
    }

    pub fn set_error_msg(&mut self, error_msg: String) {
        self.error_msg = Some(error_msg);
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn get_error_msg(&self) -> &str {
        self.error_msg.as_deref().unwrap_or("")
    }

    pub fn get_result_count(&self) -> usize {
        self.result_count
    }

    pub fn get_result_items(&self) -> &[XpathQueryResult] {
        &self.result
    }
}

pub fn eval_xpath(expr: &str, srcxml: &str) -> XpathResult {
    let mut documents = xee_xpath::Documents::new();
    let doc = match documents.add_string_without_uri(srcxml) {
        Ok(doc) => doc,
        Err(e) => {
            return XpathResult::new(
                false,
                Some(format!("Failed to parse XML: {}", e)),
                0,
                vec![],
            );
        }
    };

    let static_context_builder = match make_static_context_builder(None, &[]) {
        Ok(ctx) => ctx,
        Err(e) => {
            return XpathResult::new(
                false,
                Some(format!("Failed to build XPath context: {}", e)),
                0,
                vec![],
            );
        }
    };

    let queries = xee_xpath::Queries::new(static_context_builder);
    match execute_query(expr, &queries, &mut documents, Some(doc)) {
        Ok(res) => res,
        Err(e) => XpathResult::new(
            false,
            Some(format!("XPath query execution failed: {}", e)),
            0,
            vec![],
        ),
    }
}

fn execute_query(
    xpath: &str,
    queries: &xee_xpath::Queries<'_>,
    documents: &mut xee_xpath::Documents,
    doc: Option<xee_xpath::DocumentHandle>,
) -> Result<XpathResult, XpathEvalError> {
    let mut no_result = XpathResult::new(false, None, 0, vec![XpathQueryResult::default()]);

    let sequence_query = queries.sequence(xpath);
    let sequence_query = match sequence_query {
        Ok(sequence_query) => sequence_query,
        Err(e) => {
            let err_msg = render_error(xpath, e);
            no_result.set_success(false);
            no_result.set_error_msg(err_msg);
            return Ok(no_result);
        }
    };
    let mut context_builder = sequence_query.dynamic_context_builder(documents);
    if let Some(doc) = doc {
        context_builder.context_item(doc.to_item(documents)?);
    }
    let context = context_builder.build();

    let sequence = sequence_query.execute_with_context(documents, &context);
    let sequence = match sequence {
        Ok(sequence) => sequence,
        Err(e) => {
            let err_msg = render_error(xpath, e);
            no_result.set_success(false);
            no_result.set_error_msg(err_msg);
            return Ok(no_result);
        }
    };

    let mut results: Vec<XpathQueryResult> = Vec::new();
    for idx in 0..sequence.len() {
        let itm = sequence.get(idx).unwrap();
        let qry_result = XpathQueryResult::new(
            pretty_print_xml(
                &itm.display_representation(documents.xot(), &context)
                    .unwrap_or("error getting xpath".to_string()),
            ),
            itm.string_value(documents.xot())
                .unwrap_or("error getting string value".to_string()),
        );
        results.push(qry_result);
    }

    // construct the result
    let result = XpathResult::new(true, None, sequence.len(), results);

    Ok(result)
}

fn make_static_context_builder<'a>(
    default_namespace_uri: Option<&'a str>,
    namespaces: &'a [String],
) -> Result<StaticContextBuilder<'a>, XpathEvalError> {
    let mut static_context_builder = xee_xpath::context::StaticContextBuilder::default();
    if let Some(default_namespace_uri) = default_namespace_uri {
        static_context_builder.default_element_namespace(default_namespace_uri);
    }
    let namespaces = namespaces
        .iter()
        .map(|declaration| {
            let mut parts = declaration.splitn(2, '=');
            let prefix = parts
                .next()
                .ok_or_else(|| XpathEvalError::NamespaceDecl("missing prefix".to_string()))?;
            let uri = parts
                .next()
                .ok_or_else(|| XpathEvalError::NamespaceDecl("missing uri".to_string()))?;
            Ok((prefix, uri))
        })
        .collect::<Result<Vec<_>, XpathEvalError>>()?;

    static_context_builder.namespaces(namespaces);
    Ok(static_context_builder)
}

// ariadne error report generation

use ariadne::{Cache, CharSet, Config, IndexType, Label, Report, ReportKind, Source, Span};

fn write_ariadne_report_to_string<C: Cache<<std::ops::Range<usize> as Span>::SourceId>>(
    report: &Report,
    cache: C,
) -> String {
    let mut vec = Vec::new();
    report.write(cache, &mut vec).unwrap();
    String::from_utf8(vec).unwrap()
}

fn no_color_and_ascii() -> Config {
    Config::default()
        .with_color(false)
        // Using Ascii so that the inline snapshots display correctly
        // even with fonts where characters like '┬' take up more space.
        .with_char_set(CharSet::Ascii)
}

fn remove_trailing(s: String) -> String {
    s.lines().flat_map(|l| [l.trim_end(), "\n"]).collect()
}

fn render_error(src: &str, e: Error) -> String {
    let primary_span: SourceSpan;

    if let Some(e_span) = e.span {
        primary_span = e_span;
    } else {
        primary_span = SourceSpan::from(0..0);
    }

    let mut rpt = Report::build(ReportKind::Error, primary_span.range())
        .with_config(no_color_and_ascii().with_index_type(IndexType::Byte))
        .with_code(e.error.code())
        .with_message("invalid xpath expression");

    if let Some(span) = e.span {
        rpt = rpt.with_label(Label::new(span.range()).with_message(e.error.message()))
    }

    let rpt_final = rpt.finish();

    remove_trailing(write_ariadne_report_to_string(
        &rpt_final,
        Source::from(src),
    ))
}
