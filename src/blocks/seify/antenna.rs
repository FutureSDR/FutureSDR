/// Convert a builder antenna argument into an optional antenna name.
pub trait IntoAntenna {
    /// Convert the argument.
    fn into(self) -> Option<String>;
}

impl IntoAntenna for &str {
    fn into(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoAntenna for String {
    fn into(self) -> Option<String> {
        Some(self)
    }
}

impl IntoAntenna for Option<String> {
    fn into(self) -> Option<String> {
        self
    }
}
