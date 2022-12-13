// Boost/Apache2 License

pub enum Event<'a> {
    /// The window has just been created.
    Created,

    #[doc(hidden)]
    __NonExhaustive(&'a ()),
}
