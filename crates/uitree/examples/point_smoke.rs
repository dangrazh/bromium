//! Read-only coordinate smoke probe for the controlled integration fixture.
//! Arguments: fixture window title, x, y, expected control name.
use std::time::{Duration, Instant};

struct PointLogger;
impl log::Log for PointLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Debug
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.target().ends_with("point::diagnostics") {
            eprintln!("{}", record.args());
        }
    }
    fn flush(&self) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("BROMIUM_POINT_DIAGNOSTICS").is_some() {
        static LOGGER: PointLogger = PointLogger;
        log::set_logger(&LOGGER).expect("install fixture diagnostic logger");
        log::set_max_level(log::LevelFilter::Debug);
    }
    let args: Vec<_> = std::env::args().collect();
    let title = args.get(1).ok_or("missing fixture title")?;
    let x = args.get(2).ok_or("missing x")?.parse()?;
    let y = args.get(3).ok_or("missing y")?.parse()?;
    let expected = args.get(4).ok_or("missing expected name")?;
    let service = uitree::TreeService::new();
    service.membership(Instant::now() + Duration::from_secs(10))?;
    let started = Instant::now();
    let (tree, index) = uitree::resolve_point(
        &service,
        x,
        y,
        Some(title),
        started + Duration::from_secs(10),
    )?
    .ok_or("fixture control not found")?;
    let props = tree.node(index).1;
    assert_eq!(props.get_name(), expected);
    let xpath = tree.get_xpath_for_element(index, false)?;
    assert_eq!(tree.query(&xpath)?[0].identity(), props.identity());
    let live = service.resolve_live_expected(props.get_runtime_id(), Some(index))?;
    assert_eq!(live.get_name()?, *expected);
    println!(
        "point_ms={} name={:?} xpath={}",
        started.elapsed().as_millis(),
        expected,
        xpath
    );
    // Check the partial locator against a subsequent complete fixture-window capture.
    let refreshed = service.ensure(Some(title), Instant::now() + Duration::from_secs(10))?;
    let found = refreshed.query(&xpath)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].get_name(), expected);
    Ok(())
}
