use std::iter::FromIterator;

#[derive(Copy, Clone)]
pub struct Partial {
    /// Partial pitch.  May or may not be linear frequency.
    pub pitch: f64,
    /// Partial amplitude.  Should always be linear peak displacement.
    pub amp: f64,
}

pub struct Wave<S: AsRef<[Partial]> = Vec<Partial>>(S);

impl<S: AsRef<[Partial]>> Wave<S> {
    pub fn new(storage: S) -> Self { Self(storage) }

    pub fn iter(&self) -> impl Iterator<Item = &Partial> + Clone { self.0.as_ref().iter() }

    pub fn map_pitch<'a>(
        &'a self,
        f: impl Fn(f64) -> f64 + 'a,
    ) -> impl Iterator<Item = Partial> + 'a {
        self.0.as_ref().iter().map(move |p| Partial {
            pitch: f(p.pitch),
            ..*p
        })
    }
}

impl<S: AsRef<[Partial]>> From<S> for Wave<S> {
    fn from(s: S) -> Self { Self(s) }
}

impl<S: AsRef<[Partial]> + IntoIterator<Item = Partial>> Wave<S> {
    pub fn into_iter(self) -> S::IntoIter { self.0.into_iter() }
}

impl FromIterator<Partial> for Wave<Vec<Partial>> {
    fn from_iter<I: IntoIterator<Item = Partial>>(it: I) -> Self { Self(it.into_iter().collect()) }
}
