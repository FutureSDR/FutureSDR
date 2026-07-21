pub(crate) const fn enabled() -> bool {
    cfg!(debug_assertions)
}
