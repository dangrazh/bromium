#[macro_export]
macro_rules! printfmt {
    ($($arg:tt)*) => {
        println!("{}: {}", chrono::offset::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), format_args!($($arg)*));
    };
}
