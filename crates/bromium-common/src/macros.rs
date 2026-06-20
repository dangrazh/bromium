#[macro_export]
macro_rules! printfmt {
    ($($arg:tt)*) => {
        print!("{}: ", chrono::offset::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
        println!($($arg)*);
    };
}
