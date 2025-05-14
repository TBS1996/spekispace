#[macro_export]
macro_rules! timed {
    ($expr:expr) => {{
        let start = std::time::Instant::now();
        let result = $expr;
        let duration = start.elapsed();
        tracing::trace!("{} took {:?}", stringify!($expr), duration);
        result
    }};
    ($label:literal, $expr:expr) => {{
        let start = std::time::Instant::now();
        let result = $expr;
        let duration = start.elapsed();
        tracing::info!("{} took {:?}", $label, duration);
        result
    }};
}
