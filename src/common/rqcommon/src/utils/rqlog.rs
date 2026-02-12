// Warns and Error logs are automatically printed to console. But normal log::info() is not. But 'sometimes (at development phase)' we want that.
// Although, usually we don't want to clutter the console. Also, console is a global static with mutex, so it is slow.
// Use log_and_print() mostly for initial development phase, and as code matures and relyable, this should be gradually replaced by log() only. (not printing).
#[macro_export] // macro are better than function here, because they are not typed, and inline compiled. No function call overhead, and can accept any format string and arguments.
macro_rules! log_and_println {
    ($($arg:tt)*) => {
        log::info!($($arg)*);
        println!($($arg)*);
    };
}

#[macro_export]// macro are better than function here, because they are not typed, and inline compiled. No function call overhead, and can accept any format string and arguments.
macro_rules! log_and_if_println {
    ($is_print:expr, $($arg:tt)*) => {
        log::info!($($arg)*);
        if $is_print {
            println!($($arg)*);
        }
    };
}
