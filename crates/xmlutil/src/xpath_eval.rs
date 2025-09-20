use xee_xpath::error::Error;
use xee_xpath::context::StaticContextBuilder;
use xee_xpath::error::SourceSpan;
use xee_xpath::Itemable;
use xee_xpath::Query;
use crate::pretty_print::pretty_print_xml;

#[derive(Debug, Clone)]
pub struct XpathQueryResult {
    item_xml: String,
    item_value: String,
}

impl XpathQueryResult {
    fn new(item_xml: String, item_value: String) -> Self {
        XpathQueryResult { item_xml, item_value }
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
    fn new(success: bool, error_msg: Option<String>, result_count: usize, result: Vec<XpathQueryResult>) -> Self {
        XpathResult { success, error_msg, result_count, result }
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

    pub fn get_error_msg(&self) -> String {
        
        let mut out: String = "".to_string();
        if let Some(err_msg) = self.error_msg.as_ref() {
            out = err_msg.clone();
        }
        out
    }

    pub fn get_result_count(&self) -> usize {
        self.result_count
    }
    
    pub fn get_result_items(&self) -> Vec<XpathQueryResult> {
        self.result.clone()
    }
}



pub fn eval_xpath(expr: String, srcxml: String) -> XpathResult {
    
    let input_xml = srcxml.as_str();

    let mut documents = xee_xpath::Documents::new();
    let doc = documents.add_string_without_uri(&input_xml).unwrap();

    let static_context_builder = make_static_context_builder(
        None,
        &[],
    ).unwrap();

    let queries = xee_xpath::Queries::new(static_context_builder);
    let res = execute_query(expr.as_str(), &queries, &mut documents, Some(doc)).unwrap();
    res
}


fn execute_query(
    xpath: &str,
    queries: &xee_xpath::Queries<'_>,
    documents: &mut xee_xpath::Documents,
    doc: Option<xee_xpath::DocumentHandle>,
) -> Result<XpathResult, anyhow::Error> {

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
         pretty_print_xml(&itm.display_representation(documents.xot(), &context).unwrap_or("error getting xpath".to_string())),
        itm.string_value(documents.xot()).unwrap_or("error getting string value".to_string())
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
) -> anyhow::Result<StaticContextBuilder<'a>> {
    let mut static_context_builder = xee_xpath::context::StaticContextBuilder::default();
    if let Some(default_namespace_uri) = default_namespace_uri {
        static_context_builder.default_element_namespace(default_namespace_uri);
    }
    let namespaces = namespaces
        .iter()
        .map(|declaration| {
            let mut parts = declaration.splitn(2, '=');
            let prefix = parts.next().ok_or(anyhow::anyhow!("missing prefix"))?;
            let uri = parts.next().ok_or(anyhow::anyhow!("missing uri"))?;
            Ok((prefix, uri))
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    static_context_builder.namespaces(namespaces);
    Ok(static_context_builder)
}


// ariadne error report generation

use ariadne::{Cache, CharSet, Label, Config, Report, ReportKind, Source, Span, IndexType};


fn write_ariadne_report_to_string<C: Cache<<std::ops::Range<usize> as Span>::SourceId>>(report: &Report, cache: C) -> String {
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
        rpt = rpt.with_label(
            Label::new(span.range())
                .with_message(e.error.message())
        )
    }

    let rpt_final = rpt.finish();

    let msg = remove_trailing(write_ariadne_report_to_string(&rpt_final, Source::from(src)));

    msg
    
}

