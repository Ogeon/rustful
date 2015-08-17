use std::ops::Deref;
use std::borrow::{Cow, Borrow};
use std::hash::{Hash, Hasher};

///An owned string that may be UTF-8 encoded.
pub type MaybeUtf8Owned = MaybeUtf8<String, Vec<u8>>;
///A slice of a string that may be UTF-8 encoded.
pub type MaybeUtf8Slice<'a> = MaybeUtf8<&'a str, &'a [u8]>;

///String data that may or may not be UTF-8 encoded.
#[derive(Debug, Clone)]
pub enum MaybeUtf8<S, V> {
    ///A UTF-8 encoded string.
    Utf8(S),
    ///A non-UTF-8 string.
    NotUtf8(V)
}

impl<S, V> MaybeUtf8<S, V> {
    ///Produce a slice of this string.
    pub fn as_slice<Sref: ?Sized, Vref: ?Sized>(&self) -> MaybeUtf8<&Sref, &Vref> where S: AsRef<Sref>, V: AsRef<Vref> {
        match *self {
            MaybeUtf8::Utf8(ref s) => MaybeUtf8::Utf8(s.as_ref()),
            MaybeUtf8::NotUtf8(ref v) => MaybeUtf8::NotUtf8(v.as_ref())
        }
    }

    ///Borrow the string if it's encoded as valid UTF-8.
    pub fn as_utf8<'a>(&'a self) -> Option<&'a str> where S: AsRef<str> {
        match *self {
            MaybeUtf8::Utf8(ref s) => Some(s.as_ref()),
            MaybeUtf8::NotUtf8(_) => None
        }
    }

    ///Borrow the string if it's encoded as valid UTF-8, or make a lossy conversion.
    pub fn as_utf8_lossy<'a>(&'a self) -> Cow<'a, str> where S: AsRef<str>, V: AsRef<[u8]> {
        match *self {
            MaybeUtf8::Utf8(ref s) => s.as_ref().into(),
            MaybeUtf8::NotUtf8(ref v) => String::from_utf8_lossy(v.as_ref())
        }
    }

    ///Borrow the string as a slice of bytes.
    pub fn as_bytes(&self) -> &[u8] where S: AsRef<[u8]>, V: AsRef<[u8]> {
        match *self {
            MaybeUtf8::Utf8(ref s) => s.as_ref(),
            MaybeUtf8::NotUtf8(ref v) => v.as_ref()
        }
    }
}

impl<V> From<String> for MaybeUtf8<String, V> {
    fn from(string: String) -> MaybeUtf8<String, V> {
        MaybeUtf8::Utf8(string)
    }
}

impl<'a, V> From<&'a str> for MaybeUtf8<&'a str, V> {
    fn from(string: &'a str) -> MaybeUtf8<&'a str, V> {
        MaybeUtf8::Utf8(string)
    }
}

impl From<Vec<u8>> for MaybeUtf8<String, Vec<u8>> {
    fn from(bytes: Vec<u8>) -> MaybeUtf8<String, Vec<u8>> {
        match String::from_utf8(bytes) {
            Ok(string) => MaybeUtf8::Utf8(string),
            Err(e) => MaybeUtf8::NotUtf8(e.into_bytes())
        }
    }
}

impl<S: AsRef<[u8]>, V: AsRef<[u8]>> AsRef<[u8]> for MaybeUtf8<S, V> {
    fn as_ref(&self) -> &[u8] {
        match *self {
            MaybeUtf8::Utf8(ref s) => s.as_ref(),
            MaybeUtf8::NotUtf8(ref v) => v.as_ref()
        }
    }
}

impl<S: AsRef<[u8]>, V: AsRef<[u8]>> Borrow<[u8]> for MaybeUtf8<S, V> {
    fn borrow(&self) -> &[u8] {
        self.as_ref()
    }
}

impl<S: AsRef<[u8]>, V: AsRef<[u8]>> PartialEq for MaybeUtf8<S, V> {
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        self.as_ref().eq(other.as_ref())
    }
}

impl<S: AsRef<[u8]>, V: AsRef<[u8]>> Eq for MaybeUtf8<S, V> {}

impl<S: AsRef<[u8]>, V: AsRef<[u8]>> Hash for MaybeUtf8<S, V> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.as_ref().hash(hasher)
    }
}

impl<S: Into<String>, V: Into<Vec<u8>>> Into<String> for MaybeUtf8<S, V> {
    fn into(self) -> String {
        match self {
            MaybeUtf8::Utf8(s) => s.into(),
            MaybeUtf8::NotUtf8(v) => {
                let bytes = v.into();
                match String::from_utf8_lossy(&bytes) {
                    Cow::Borrowed(_) => unsafe { String::from_utf8_unchecked(bytes) },
                    Cow::Owned(s) => s
                }
            }
        }
    }
}

impl<S: Into<Vec<u8>>, V: Into<Vec<u8>>> Into<Vec<u8>> for MaybeUtf8<S, V> {
    fn into(self) -> Vec<u8> {
        match self {
            MaybeUtf8::Utf8(s) => s.into(),
            MaybeUtf8::NotUtf8(v) => v.into()
        }
    }
}

impl<S: AsRef<[u8]>, V: AsRef<[u8]>> Deref for MaybeUtf8<S, V> {
    type Target=[u8];

    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}