use std::collections::hash_map::{HashMap, Entry};
use std::iter::FromIterator;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use std::hash::Hash;
use std::borrow::Cow;

use context::MaybeUtf8Owned;

///An extended `HashMap` with extra functionality for value parsing.
///
///Some of the methods from `HashMap` has been wrapped to provide a more
///ergonomic API, where anything that can be represented as a byte slice can
///be used as a key.
#[derive(Clone)]
pub struct Parameters(HashMap<MaybeUtf8Owned, MaybeUtf8Owned>);

impl Parameters {
    ///Create an empty `Parameters`.
    pub fn new() -> Parameters {
        Parameters(HashMap::new())
    }

    ///Get a parameter as a UTF-8 string. A lossy conversion will be performed
    ///if it's not encoded as UTF-8. Use `get_raw` to get the original data.
    pub fn get<'a, K: ?Sized>(&'a self, key: &K) -> Option<Cow<'a, str>> where
        K: Hash + Eq + AsRef<[u8]>
    {
        self.0.get(key.as_ref()).map(|v| v.as_utf8_lossy())
    }

    ///Get a parameter that may or may not be a UTF-8 string.
    pub fn get_raw<'a, K: ?Sized>(&'a self, key: &K) -> Option<&'a MaybeUtf8Owned> where
        K: Hash + Eq + AsRef<[u8]>
    {
        self.0.get(key.as_ref())
    }

    ///Get a mutable parameter that may or may not be a UTF-8 string.
    pub fn get_mut<'a, K: ?Sized>(&'a mut self, key: &K) -> Option<&'a mut MaybeUtf8Owned> where
        K: Hash + Eq + AsRef<[u8]>
    {
        self.0.get_mut(key.as_ref())
    }

    ///Returns true if a parameter with the given key exists.
    pub fn contains_key<K: ?Sized>(&self, key: &K) -> bool where
        K: Hash + Eq + AsRef<[u8]>
    {
        self.0.contains_key(key.as_ref())
    }

    ///Insert a parameter.
    pub fn insert<K, V>(&mut self, key: K, value: V) -> Option<MaybeUtf8Owned> where
        K: Into<MaybeUtf8Owned>, V: Into<MaybeUtf8Owned>
    {
        self.0.insert(key.into(), value.into())
    }

    ///Remove a parameter and return it.
    pub fn remove<K: ?Sized>(&mut self, key: &K) -> Option<MaybeUtf8Owned> where
        K: Hash + Eq + AsRef<[u8]>
    {
        self.0.remove(key.as_ref())
    }

    ///Gets the given key's corresponding parameter in the map for in-place
    ///manipulation.
    pub fn entry<K>(&mut self, key: K) -> Entry<MaybeUtf8Owned, MaybeUtf8Owned> where K: Into<MaybeUtf8Owned> {
        self.0.entry(key.into())
    }

    ///Try to parse an entry as `T`, if it exists. The error will be `None` if
    ///the entry does not exist, and `Some` if it does exists, but the parsing
    ///failed.
    ///
    ///```
    ///# use rustful::{Context, Response};
    ///fn my_handler(context: Context, response: Response) {
    ///    let age: Result<u8, _> = context.variables.parse("age");
    ///    match age {
    ///        Ok(age) => response.send(format!("age: {}", age)),
    ///        Err(Some(_)) => response.send("age must be a positive number"),
    ///        Err(None) => response.send("no age provided")
    ///    }
    ///}
    ///```
    pub fn parse<K: ?Sized, T>(&self, key: &K) -> Result<T, Option<T::Err>> where
        K: Hash + Eq + AsRef<[u8]>,
        T: FromStr
    {
        if let Some(val) = self.0.get(key.as_ref()) {
            val.as_utf8_lossy().parse().map_err(|e| Some(e))
        } else {
            Err(None)
        }
    }

    ///Try to parse an entry as `T`, if it exists, or return the default in
    ///`or`.
    ///
    ///```
    ///# use rustful::{Context, Response};
    ///fn my_handler(context: Context, response: Response) {
    ///    let page = context.variables.parse_or("page", 0u8);
    ///    response.send(format!("current page: {}", page));
    ///}
    ///```
    pub fn parse_or<K: ?Sized, T>(&self, key: &K, or: T) -> T where
        K: Hash + Eq + AsRef<[u8]>,
        T: FromStr
    {
        self.parse(key).unwrap_or(or)
    }

    ///Try to parse an entry as `T`, if it exists, or create a new one using
    ///`or_else`. The `or_else` function will receive the parsing error if the
    ///value existed, but was impossible to parse.
    ///
    ///```
    ///# use rustful::{Context, Response};
    ///# fn do_heavy_stuff() -> u8 {0}
    ///fn my_handler(context: Context, response: Response) {
    ///    let science = context.variables.parse_or_else("science", |_| do_heavy_stuff());
    ///    response.send(format!("science value: {}", science));
    ///}
    ///```
    pub fn parse_or_else<K: ?Sized, T, F>(&self, key: &K, or_else: F) -> T where
        K: Hash + Eq + AsRef<[u8]>,
        T: FromStr,
        F: FnOnce(Option<T::Err>) -> T
    {
        self.parse(key).unwrap_or_else(or_else)
    }
}

impl Deref for Parameters {
    type Target = HashMap<MaybeUtf8Owned, MaybeUtf8Owned>;

    fn deref(&self) -> &HashMap<MaybeUtf8Owned, MaybeUtf8Owned> {
        &self.0
    }
}

impl DerefMut for Parameters {
    fn deref_mut(&mut self) -> &mut HashMap<MaybeUtf8Owned, MaybeUtf8Owned> {
        &mut self.0
    }
}

impl AsRef<HashMap<MaybeUtf8Owned, MaybeUtf8Owned>> for Parameters {
    fn as_ref(&self) -> &HashMap<MaybeUtf8Owned, MaybeUtf8Owned> {
        &self.0
    }
}

impl AsMut<HashMap<MaybeUtf8Owned, MaybeUtf8Owned>> for Parameters {
    fn as_mut(&mut self) -> &mut HashMap<MaybeUtf8Owned, MaybeUtf8Owned> {
        &mut self.0
    }
}

impl Into<HashMap<MaybeUtf8Owned, MaybeUtf8Owned>> for Parameters {
    fn into(self) -> HashMap<MaybeUtf8Owned, MaybeUtf8Owned> {
        self.0
    }
}

impl From<HashMap<MaybeUtf8Owned, MaybeUtf8Owned>> for Parameters {
    fn from(map: HashMap<MaybeUtf8Owned, MaybeUtf8Owned>) -> Parameters {
        Parameters(map)
    }
}

impl PartialEq for Parameters {
    fn eq(&self, other: &Parameters) -> bool {
        self.0.eq(&other.0)
    }
}

impl Eq for Parameters {}

impl fmt::Debug for Parameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Default for Parameters {
    fn default() -> Parameters {
        Parameters::new()
    }
}

impl IntoIterator for Parameters {
    type IntoIter = <HashMap<MaybeUtf8Owned, MaybeUtf8Owned> as IntoIterator>::IntoIter;
    type Item = (MaybeUtf8Owned, MaybeUtf8Owned);

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Parameters {
    type IntoIter = <&'a HashMap<MaybeUtf8Owned, MaybeUtf8Owned> as IntoIterator>::IntoIter;
    type Item = (&'a MaybeUtf8Owned, &'a MaybeUtf8Owned);

    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

impl<'a> IntoIterator for &'a mut Parameters {
    type IntoIter = <&'a mut HashMap<MaybeUtf8Owned, MaybeUtf8Owned> as IntoIterator>::IntoIter;
    type Item = (&'a MaybeUtf8Owned, &'a mut MaybeUtf8Owned);

    fn into_iter(self) -> Self::IntoIter {
        (&mut self.0).into_iter()
    }
}

impl<K: Into<MaybeUtf8Owned>, V: Into<MaybeUtf8Owned>> FromIterator<(K, V)> for Parameters {
    fn from_iter<T: IntoIterator<Item=(K, V)>>(iterable: T) -> Parameters {
        HashMap::from_iter(iterable.into_iter().map(|(k, v)| (k.into(), v.into()))).into()
    }
}

impl<K: Into<MaybeUtf8Owned>, V: Into<MaybeUtf8Owned>> Extend<(K, V)> for Parameters {
    fn extend<T: IntoIterator<Item=(K, V)>>(&mut self, iter: T) {
        self.0.extend(iter.into_iter().map(|(k, v)| (k.into(), v.into())))
    }
}