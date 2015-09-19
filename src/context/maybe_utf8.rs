use std::ops::{Deref, DerefMut, Drop};
use std::borrow::{Cow, Borrow};
use std::hash::{Hash, Hasher};

use ::utils::BytesExt;

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
    ///Create an empty UTF-8 string.
    pub fn new() -> MaybeUtf8<S, V> where S: From<&'static str> {
        MaybeUtf8::Utf8("".into())
    }

    ///Produce a slice of this string.
    ///
    ///```
    ///use rustful::context::{MaybeUtf8Owned, MaybeUtf8Slice};
    ///
    ///let owned = MaybeUtf8Owned::from("abc");
    ///let slice: MaybeUtf8Slice = owned.as_slice();
    ///```
    pub fn as_slice<Sref: ?Sized, Vref: ?Sized>(&self) -> MaybeUtf8<&Sref, &Vref> where S: AsRef<Sref>, V: AsRef<Vref> {
        match *self {
            MaybeUtf8::Utf8(ref s) => MaybeUtf8::Utf8(s.as_ref()),
            MaybeUtf8::NotUtf8(ref v) => MaybeUtf8::NotUtf8(v.as_ref())
        }
    }

    ///Borrow the string if it's encoded as valid UTF-8.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let string = MaybeUtf8Owned::from("abc");
    ///assert_eq!(Some("abc"), string.as_utf8());
    ///```
    pub fn as_utf8<'a>(&'a self) -> Option<&'a str> where S: AsRef<str> {
        match *self {
            MaybeUtf8::Utf8(ref s) => Some(s.as_ref()),
            MaybeUtf8::NotUtf8(_) => None
        }
    }

    ///Borrow the string if it's encoded as valid UTF-8, or make a lossy conversion.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let string = MaybeUtf8Owned::from("abc");
    ///assert_eq!("abc", string.as_utf8_lossy());
    ///```
    pub fn as_utf8_lossy<'a>(&'a self) -> Cow<'a, str> where S: AsRef<str>, V: AsRef<[u8]> {
        match *self {
            MaybeUtf8::Utf8(ref s) => s.as_ref().into(),
            MaybeUtf8::NotUtf8(ref v) => String::from_utf8_lossy(v.as_ref())
        }
    }

    ///Borrow the string as a slice of bytes.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let string = MaybeUtf8Owned::from("abc");
    ///assert_eq!(b"abc", string.as_bytes());
    ///```
    pub fn as_bytes(&self) -> &[u8] where S: AsRef<[u8]>, V: AsRef<[u8]> {
        match *self {
            MaybeUtf8::Utf8(ref s) => s.as_ref(),
            MaybeUtf8::NotUtf8(ref v) => v.as_ref()
        }
    }

    ///Check if the string is valid UTF-8.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let valid = MaybeUtf8Owned::from("abc");
    ///assert_eq!(valid.is_utf8(), true);
    ///
    ///let invalid = MaybeUtf8Owned::from(vec![255]);
    ///assert_eq!(invalid.is_utf8(), false);
    ///```
    pub fn is_utf8(&self) -> bool {
        match *self {
            MaybeUtf8::Utf8(_) => true,
            MaybeUtf8::NotUtf8(_) => false
        }
    }
}

impl MaybeUtf8<String, Vec<u8>> {
    ///Push a single `char` to the end of the string.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let mut string = MaybeUtf8Owned::from("abc");
    ///string.push_char('d');
    ///assert_eq!("abcd", string);
    ///```
    pub fn push_char(&mut self, c: char) {
        match *self {
            MaybeUtf8::Utf8(ref mut s) => s.push(c),
            MaybeUtf8::NotUtf8(ref mut v) => {
                //Do some witchcraft until encode_utf8 becomes a thing.
                let string: &mut String = unsafe { ::std::mem::transmute(v) };
                string.push(c);
            }
        }
    }

    ///Push a single byte to the end of the string. The string's UTF-8
    ///compatibility will be reevaluated and may change each time `push_byte`
    ///is called. This may have a noticeable performance impact.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let mut string = MaybeUtf8Owned::from("abc");
    ///string.push_byte(255);
    ///assert_eq!(string.is_utf8(), false);
    ///```
    pub fn push_byte(&mut self, byte: u8) {
        self.push_bytes(&[byte])
    }

    ///Extend the string.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let mut string = MaybeUtf8Owned::from("abc");
    ///string.push_str("def");
    ///assert_eq!("abcdef", string);
    ///```
    pub fn push_str(&mut self, string: &str) {
        match *self {
            MaybeUtf8::Utf8(ref mut s) => s.push_str(string),
            MaybeUtf8::NotUtf8(ref mut v) => v.push_bytes(string.as_bytes())
        }
    }

    ///Push a number of bytes to the string. The string's UTF-8 compatibility
    ///may change.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let mut string = MaybeUtf8Owned::from("abc");
    ///string.push_bytes(&[100, 101, 102]);
    ///assert_eq!("abcdef", string);
    ///```
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        match ::std::str::from_utf8(bytes) {
            Ok(string) => self.push_str(string),
            Err(_) => {
                self.as_buffer().push_bytes(bytes);
            }
        }
    }

    ///Borrow this string as a mutable byte buffer. The string's UTF-8
    ///compatibility will be reevaluated when the buffer is dropped.
    pub fn as_buffer(&mut self) -> Buffer {
        let mut v = MaybeUtf8::NotUtf8(vec![]);
        ::std::mem::swap(self, &mut v);
        Buffer {
            bytes: v.into(),
            source: self
        }
    }
}

impl<V> From<String> for MaybeUtf8<String, V> {
    fn from(string: String) -> MaybeUtf8<String, V> {
        MaybeUtf8::Utf8(string)
    }
}

impl<'a, S: From<&'a str>, V> From<&'a str> for MaybeUtf8<S, V> {
    fn from(string: &'a str) -> MaybeUtf8<S, V> {
        MaybeUtf8::Utf8(string.into())
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

impl<S, V, B: ?Sized> PartialEq<B> for MaybeUtf8<S, V> where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>,
    B: AsRef<[u8]>
{
    fn eq(&self, other: &B) -> bool {
        self.as_ref().eq(other.as_ref())
    }
}

impl<S, V> PartialEq<MaybeUtf8<S, V>> for str where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>
{
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        other.eq(self)
    }
}

impl<'a, S, V> PartialEq<MaybeUtf8<S, V>> for &'a str where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>
{
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        other.eq(self)
    }
}

impl<S, V> PartialEq<MaybeUtf8<S, V>> for String where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>
{
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        other.eq(self)
    }
}

impl<'a, S, V> PartialEq<MaybeUtf8<S, V>> for Cow<'a, str> where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>
{
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        other.eq(self.as_ref())
    }
}

impl<S, V> PartialEq<MaybeUtf8<S, V>> for [u8] where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>
{
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        other.eq(self)
    }
}

impl<'a, S, V> PartialEq<MaybeUtf8<S, V>> for &'a [u8] where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>
{
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        other.eq(self)
    }
}

impl<S, V> PartialEq<MaybeUtf8<S, V>> for Vec<u8> where
    S: AsRef<[u8]>,
    V: AsRef<[u8]>
{
    fn eq(&self, other: &MaybeUtf8<S, V>) -> bool {
        other.eq(self)
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

///A byte buffer for more efficient `MaybeUtf8` manipulation.
///
///The buffer is essentially a `&mut Vec<u8>` that will be checked for UTF-8
///compatibility when dropped. It comes with a few extra convenience methods.
pub struct Buffer<'a> {
    bytes: Vec<u8>,
    source: &'a mut MaybeUtf8<String, Vec<u8>>
}

impl<'a> Buffer<'a> {
    ///Push a number of bytes to the buffer in a relatively efficient way.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let mut string = MaybeUtf8Owned::new();
    ///
    ///{
    ///    let mut buffer = string.as_buffer();
    ///    buffer.push_bytes("abc".as_bytes());
    ///}
    ///
    ///assert!(string.is_utf8());
    ///assert_eq!("abc", string);
    ///```
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        self.bytes.push_bytes(bytes)
    }

    ///Push a single `char` to the end of the buffer.
    ///
    ///```
    ///use rustful::context::MaybeUtf8Owned;
    ///
    ///let mut string = MaybeUtf8Owned::new();
    ///
    ///{
    ///    let mut buffer = string.as_buffer();
    ///    buffer.push_char('å');
    ///    buffer.push_char('1');
    ///    buffer.push_char('€');
    ///}
    ///
    ///assert!(string.is_utf8());
    ///assert_eq!("å1€", string);
    ///```
    pub fn push_char(&mut self, c: char) {
        //Do some witchcraft until encode_utf8 becomes a thing.
        let string: &mut String = unsafe { ::std::mem::transmute(&mut self.bytes) };
        string.push(c);
    }
}

impl<'a> Deref for Buffer<'a> {
    type Target = Vec<u8>;

    fn deref(&self) -> &Vec<u8> {
        &self.bytes
    }
}

impl<'a> DerefMut for Buffer<'a> {
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        &mut self.bytes
    }
}

impl<'a> Drop for Buffer<'a> {
    fn drop(&mut self) {
        let mut v = vec![];
        ::std::mem::swap(&mut v, &mut self.bytes);
        *self.source = v.into();
    }
}
