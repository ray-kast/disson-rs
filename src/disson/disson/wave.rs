use std::iter::FromIterator;

pub struct Partial {
    /// Partial pitch.  May or may not be linear frequency.
    pub pitch: f64,
    /// Partial amplitude.  Should always be linear peak displacement.
    pub amp: f64,
}

pub struct Wave<S: AsRef<[Partial]>>(S);

impl<S: AsRef<[Partial]>> Wave<S> {
    pub fn new(storage: S) -> Self { Self(storage) }

    pub fn iter(&self) -> impl Iterator<Item = &Partial> + Clone { self.0.as_ref().iter() }
}

impl FromIterator<Partial> for Wave<Vec<Partial>> {
    fn from_iter<I: IntoIterator<Item = Partial>>(it: I) -> Self { Self(it.into_iter().collect()) }
}
