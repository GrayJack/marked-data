//! Serde support for marked data deserialisation

use std::{
    fmt,
    hash::Hash,
    iter::Peekable,
    marker::PhantomData,
    num::{ParseFloatError, ParseIntError},
    ops::Deref,
};

use serde::{
    de::{value::BorrowedStrDeserializer, IntoDeserializer, MapAccess, SeqAccess, Visitor},
    forward_to_deserialize_any, Deserialize, Deserializer, Serialize,
};

use crate::{
    types::{MarkedMappingNode, MarkedScalarNode, MarkedSequenceNode},
    Marker, Node, Span,
};

/// Wrapper which can be used when deserialising data from [`Node`]
///
/// You must use a compatible deserializer if you want to deserialize these values,
/// however when serializing you will lose the span information so do not expect
/// to round-trip these values.
#[derive(Debug)]
pub struct Spanned<T> {
    span: Span,
    inner: T,
}

impl<T> Spanned<T> {
    /// Wrap an instance of something with the given span
    pub fn new(span: Span, inner: T) -> Self {
        Self { span, inner }
    }

    /// The span associated with this value
    pub fn span(&self) -> &Span {
        &self.span
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> PartialEq for Spanned<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T> Eq for Spanned<T> where T: Eq {}

impl<T> Hash for Spanned<T>
where
    T: Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

// -------------------------------------------------------------------------------

// Convention for these markers comes from the toml crates

const SPANNED_TYPE: &str = "$___::marked_data::serde::Spanned<T>";
const SPANNED_SPAN_START_SOURCE: &str = "$___::marked_data::serde::Spanned<T>::span_start_source";
const SPANNED_SPAN_START_LINE: &str = "$___::marked_data::serde::Spanned<T>::span_start_line";
const SPANNED_SPAN_START_COLUMN: &str = "$___::marked_data::serde::Spanned<T>::span_start_column";
const SPANNED_SPAN_END_SOURCE: &str = "$___::marked_data::serde::Spanned<T>::span_end_source";
const SPANNED_SPAN_END_LINE: &str = "$___::marked_data::serde::Spanned<T>::span_end_line";
const SPANNED_SPAN_END_COLUMN: &str = "$___::marked_data::serde::Spanned<T>::span_end_column";
const SPANNED_INNER: &str = "$___::marked_data::serde::Spanned<T>::inner";

const SPANNED_FIELDS: [&str; 7] = [
    SPANNED_SPAN_START_SOURCE,
    SPANNED_SPAN_START_LINE,
    SPANNED_SPAN_START_COLUMN,
    SPANNED_SPAN_END_SOURCE,
    SPANNED_SPAN_END_LINE,
    SPANNED_SPAN_END_COLUMN,
    SPANNED_INNER,
];

impl<'de, T> Deserialize<'de> for Spanned<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MarkedNodeVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for MarkedNodeVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = Spanned<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a MarkedNode of some kind")
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut key: Option<&str> = visitor.next_key()?;

                let span_start = if key == Some(SPANNED_SPAN_START_SOURCE) {
                    let source: usize = visitor.next_value()?;
                    if visitor.next_key()? != Some(SPANNED_SPAN_START_LINE) {
                        return Err(serde::de::Error::custom(
                            "marked node span start line missing",
                        ));
                    }
                    let line: usize = visitor.next_value()?;
                    if visitor.next_key()? != Some(SPANNED_SPAN_START_COLUMN) {
                        return Err(serde::de::Error::custom(
                            "marked node span start column missing",
                        ));
                    }
                    let column: usize = visitor.next_value()?;
                    key = visitor.next_key()?;
                    Some(Marker::new(source, line, column))
                } else {
                    None
                };

                let span_end = if key == Some(SPANNED_SPAN_END_SOURCE) {
                    let source: usize = visitor.next_value()?;
                    if visitor.next_key()? != Some(SPANNED_SPAN_END_LINE) {
                        return Err(serde::de::Error::custom(
                            "marked node span end line missing",
                        ));
                    }
                    let line: usize = visitor.next_value()?;
                    if visitor.next_key()? != Some(SPANNED_SPAN_END_COLUMN) {
                        return Err(serde::de::Error::custom(
                            "marked node span end column missing",
                        ));
                    }
                    let column: usize = visitor.next_value()?;
                    key = visitor.next_key()?;
                    Some(Marker::new(source, line, column))
                } else {
                    None
                };

                if key != Some(SPANNED_INNER) {
                    return Err(serde::de::Error::custom(
                        "marked node inner value not found",
                    ));
                }
                let inner: T = visitor.next_value()?;

                let mut span = Span::new_blank();
                span.set_start(span_start);
                span.set_end(span_end);

                Ok(Spanned::new(span, inner))
            }
        }
        let visitor = MarkedNodeVisitor(PhantomData);

        deserializer.deserialize_struct(SPANNED_TYPE, &SPANNED_FIELDS, visitor)
    }
}

impl<T> Serialize for Spanned<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

// -------------------------------------------------------------------------------

/// Errors which can come from deserialisation
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// The value was not a valid boolean
    NotBoolean(Span),
    /// Failed to parse integer
    IntegerParseFailure(ParseIntError, Span),
    /// Failed to parse float
    FloatParseFailure(ParseFloatError, Span),
    /// An unknown field was encountered
    UnknownFieldError(String, &'static [&'static str], Span),
    /// Some other error occurred
    Other(Box<dyn std::error::Error>, Span),
}

impl Error {
    fn set_span(&mut self, span: Span) {
        let spanloc = match self {
            Error::NotBoolean(s) => s,
            Error::IntegerParseFailure(_, s) => s,
            Error::FloatParseFailure(_, s) => s,
            Error::UnknownFieldError(_, _, s) => s,
            Error::Other(_, s) => s,
        };
        *spanloc = span;
    }

    /// Retrieve the start marker if there is one
    ///
    /// Most spans which are generated by the loader only have start
    /// marks (containers have end marks as well, but these failures)
    /// are unlikely to exist here.
    ///
    /// ```
    /// # use marked_yaml::*;
    /// # use serde::Deserialize;
    /// const YAML: &str = r#"
    /// bad: float
    /// "#;
    ///
    /// #[derive(Deserialize)]
    /// struct Example {
    ///     bad: Spanned<f64>,
    /// }
    ///
    /// let nodes = parse_yaml(0, YAML).unwrap();
    /// let err = from_node::<Example>(&nodes).err().unwrap();
    #[cfg_attr(
        feature = "serde-path",
        doc = "// Extract our error from the path-to-error\n// Not necessary if not using the serde-path feature\nlet err = err.into_inner();"
    )]
    #[cfg_attr(
        not(feature = "serde-path"),
        doc = "// If using the serde-path feature, you would need to\n// extract our error from the path-to-error\n// let err = err.into_inner();"
    )]
    ///
    /// assert!(matches!(err, Error::FloatParseFailure(_,_)));
    ///
    /// let mark = err.start_mark().unwrap();
    ///
    /// assert_eq!(mark.source(), 0);
    /// assert_eq!(mark.line(), 2);
    /// assert_eq!(mark.column(), 6);
    /// ```
    pub fn start_mark(&self) -> Option<Marker> {
        let spanloc = match self {
            Error::NotBoolean(s) => s,
            Error::IntegerParseFailure(_, s) => s,
            Error::FloatParseFailure(_, s) => s,
            Error::UnknownFieldError(_, _, s) => s,
            Error::Other(_, s) => s,
        };
        spanloc.start().copied()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotBoolean(_) => f.write_str("Value was not a boolean"),
            Error::IntegerParseFailure(e, _) => e.fmt(f),
            Error::FloatParseFailure(e, _) => e.fmt(f),
            Error::UnknownFieldError(field, expected, _) => match expected.len() {
                0 => write!(f, "Unknown field `{field}`, there are no fields"),
                1 => write!(f, "Unknown field `{field}`, expected `{}`", expected[0]),
                2 => write!(
                    f,
                    "Unknown field `{field}`, expected `{}` or `{}`",
                    expected[0], expected[1]
                ),
                _ => {
                    write!(f, "Unknown field `{field}`, expected one of ")?;
                    let last = expected[expected.len() - 1];
                    for v in expected[..=expected.len() - 2].iter() {
                        write!(f, "`{v}`, ")?;
                    }
                    write!(f, "or `{last}`")
                }
            },
            Error::Other(e, _) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Error::Other(msg.to_string().into(), Span::new_blank())
    }

    fn unknown_field(field: &str, expected: &'static [&'static str]) -> Self {
        Self::UnknownFieldError(field.to_string(), expected, Span::new_blank())
    }
}

impl From<ParseIntError> for Error {
    fn from(value: ParseIntError) -> Self {
        Error::IntegerParseFailure(value, Span::new_blank())
    }
}

impl From<ParseFloatError> for Error {
    fn from(value: ParseFloatError) -> Self {
        Error::FloatParseFailure(value, Span::new_blank())
    }
}

trait AddSpans<T> {
    fn addspans(self, span: Span) -> Result<T, Error>;
}

impl<T, E> AddSpans<T> for Result<T, E>
where
    E: Into<Error>,
{
    fn addspans(self, span: Span) -> Result<T, Error> {
        self.map_err(|e| {
            let mut e: Error = e.into();
            e.set_span(span);
            e
        })
    }
}

// -------------------------------------------------------------------------------

impl<'de> IntoDeserializer<'de, Error> for &'de Node {
    type Deserializer = NodeDeserializer<'de>;

    fn into_deserializer(self) -> Self::Deserializer {
        NodeDeserializer { node: self }
    }
}

/// Deserializer for nodes
pub struct NodeDeserializer<'node> {
    node: &'node Node,
}

impl<'node> NodeDeserializer<'node> {
    /// Create a new deserializer over a borrowed node
    pub fn new(node: &'node Node) -> Self {
        Self { node }
    }
}

#[cfg(not(feature = "serde-path"))]
pub type FromNodeError = Error;

#[cfg(feature = "serde-path")]
pub type FromNodeError = serde_path_to_error::Error<Error>;

/// Deserialize some [`Node`] into the requisite type
///
/// This permits deserialisation of [`Node`]s into any structure
/// which [`serde`] can deserialize.  In addition, if any part of
/// the type tree is [`Spanned`] then the spans are provided
/// from the requisite marked node.
///
/// ```
/// # use serde::Deserialize;
/// # use marked_yaml::Spanned;
/// const YAML: &str = "hello: world\n";
/// let node = marked_yaml::parse_yaml(0, YAML).unwrap();
/// #[derive(Deserialize)]
/// struct Greeting {
///     hello: Spanned<String>,
/// }
/// let greets: Greeting = marked_yaml::from_node(&node).unwrap();
/// let start = greets.hello.span().start().unwrap();
/// assert_eq!(start.line(), 1);
/// assert_eq!(start.column(), 8);
/// ```
#[allow(clippy::result_large_err)]
pub fn from_node<'de, T>(node: &'de Node) -> Result<T, FromNodeError>
where
    T: Deserialize<'de>,
{
    #[cfg(not(feature = "serde-path"))]
    fn inner_from_node<'de, T>(node: &'de Node) -> Result<T, Error>
    where
        T: Deserialize<'de>,
    {
        T::deserialize(NodeDeserializer::new(node))
    }

    #[cfg(feature = "serde-path")]
    fn inner_from_node<'de, T>(node: &'de Node) -> Result<T, serde_path_to_error::Error<Error>>
    where
        T: Deserialize<'de>,
    {
        use serde_path_to_error::Segment;

        let p2e: Result<T, _> = serde_path_to_error::deserialize(NodeDeserializer::new(node));

        p2e.map_err(|e| {
            if e.inner().start_mark().is_none() {
                let p = e.path().clone();
                let mut e = e.into_inner();
                let mut prev_best_node = node;
                let mut best_node = node;
                for seg in p.iter() {
                    match seg {
                        Segment::Seq { index } => {
                            if let Some(seq) = best_node.as_sequence() {
                                if let Some(node) = seq.get(*index) {
                                    prev_best_node = best_node;
                                    best_node = node;
                                } else {
                                    // We can't traverse this?
                                    break;
                                }
                            } else {
                                // We can't traverse this?
                                break;
                            }
                        }
                        Segment::Map { key } => {
                            if let Some(map) = best_node.as_mapping() {
                                // What we want here is the entry which matches the key
                                // if there is one
                                if let Some(node) = map.get(key.as_str()) {
                                    prev_best_node = best_node;
                                    best_node = node;
                                } else {
                                    // We can't traverse this?
                                    break;
                                }
                            } else {
                                // We can't traverse this?
                                break;
                            }
                        }
                        Segment::Enum { .. } => break,
                        Segment::Unknown => break,
                    }
                }
                let mut best_span = *best_node.span();
                if let Error::UnknownFieldError(field, _, _) = &e {
                    // We actually would prefer to point at the key not the value,
                    if let Some(map) = prev_best_node.as_mapping() {
                        for (k, _) in map.iter() {
                            if k.as_str() == field.as_str() {
                                best_span = *k.span();
                                break;
                            }
                        }
                    }
                }
                e.set_span(best_span);
                serde_path_to_error::Error::new(p, e)
            } else {
                e
            }
        })
    }

    inner_from_node(node)
}

macro_rules! forward_to_nodes {
    () => {
        forward_to_nodes! [
            deserialize_any()
            deserialize_bool()
            deserialize_i8()
            deserialize_i16()
            deserialize_i32()
            deserialize_i64()
            deserialize_i128()
            deserialize_u8()
            deserialize_u16()
            deserialize_u32()
            deserialize_u64()
            deserialize_u128()
            deserialize_f32()
            deserialize_f64()
            deserialize_char()
            deserialize_str()
            deserialize_string()
            deserialize_bytes()
            deserialize_byte_buf()
            deserialize_option()
            deserialize_unit()
            deserialize_unit_struct(name: &'static str)
            deserialize_newtype_struct(name: &'static str)
            deserialize_seq()
            deserialize_tuple(len: usize)
            deserialize_tuple_struct(name: &'static str, len: usize)
            deserialize_map()
            deserialize_struct(name: &'static str, fields: &'static [&'static str])
            deserialize_enum(name: &'static str, variants: &'static [&'static str])
            deserialize_identifier()
            deserialize_ignored_any()
        ];
    };

    ($($meth:ident($($arg:ident: $ty:ty),*))*) => {
        $(
            fn $meth<V>(self, $($arg: $ty,)* visitor: V) -> Result<V::Value, Self::Error>
            where
              V: Visitor<'de>,
            {
                match self.node {
                    Node::Scalar(s) => s
                        .into_deserializer()
                        .$meth($($arg,)* visitor),
                    Node::Mapping(m) => m
                        .into_deserializer()
                        .$meth($($arg,)* visitor),
                    Node::Sequence(s) => s
                        .into_deserializer()
                        .$meth($($arg,)* visitor),
                }
            }
        )*
    };
}

impl<'de> Deserializer<'de> for NodeDeserializer<'de> {
    type Error = Error;

    forward_to_nodes!();
}

// -------------------------------------------------------------------------------

trait MarkedValue {
    fn mark_span(&self) -> &Span;
}

impl MarkedValue for MarkedScalarNode {
    fn mark_span(&self) -> &Span {
        self.span()
    }
}

impl MarkedValue for MarkedMappingNode {
    fn mark_span(&self) -> &Span {
        self.span()
    }
}

impl MarkedValue for MarkedSequenceNode {
    fn mark_span(&self) -> &Span {
        self.span()
    }
}

impl MarkedValue for Node {
    fn mark_span(&self) -> &Span {
        self.span()
    }
}

// -------------------------------------------------------------------------------

struct SpannedDeserializer<'de, T> {
    node: &'de T,
    state: SpannedDeserializerState,
}

enum SpannedDeserializerState {
    SendStartSource,
    SendStartLine,
    SendStartColumn,
    SendEndSource,
    SendEndLine,
    SendEndColumn,
    SendValue,
    Done,
}

impl<'de, T> SpannedDeserializer<'de, T>
where
    T: MarkedValue,
{
    fn new(node: &'de T) -> Self {
        let state = if node.mark_span().start().is_some() {
            SpannedDeserializerState::SendStartSource
        } else if node.mark_span().end().is_some() {
            SpannedDeserializerState::SendEndSource
        } else {
            SpannedDeserializerState::SendValue
        };
        Self { node, state }
    }
}

impl<'de, T> MapAccess<'de> for SpannedDeserializer<'de, T>
where
    T: MarkedValue,
    &'de T: IntoDeserializer<'de, Error>,
{
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        let key = match self.state {
            SpannedDeserializerState::SendStartSource => SPANNED_SPAN_START_SOURCE,
            SpannedDeserializerState::SendStartLine => SPANNED_SPAN_START_LINE,
            SpannedDeserializerState::SendStartColumn => SPANNED_SPAN_START_COLUMN,
            SpannedDeserializerState::SendEndSource => SPANNED_SPAN_END_SOURCE,
            SpannedDeserializerState::SendEndLine => SPANNED_SPAN_END_LINE,
            SpannedDeserializerState::SendEndColumn => SPANNED_SPAN_END_COLUMN,
            SpannedDeserializerState::SendValue => SPANNED_INNER,
            SpannedDeserializerState::Done => return Ok(None),
        };
        seed.deserialize(BorrowedStrDeserializer::new(key))
            .map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        match self.state {
            SpannedDeserializerState::SendStartSource => {
                let v = self
                    .node
                    .mark_span()
                    .start()
                    .expect("Span missing start")
                    .source();
                self.state = SpannedDeserializerState::SendStartLine;
                seed.deserialize(v.into_deserializer())
            }
            SpannedDeserializerState::SendStartLine => {
                let v = self
                    .node
                    .mark_span()
                    .start()
                    .expect("Span missing start")
                    .line();
                self.state = SpannedDeserializerState::SendStartColumn;
                seed.deserialize(v.into_deserializer())
            }
            SpannedDeserializerState::SendStartColumn => {
                let v = self
                    .node
                    .mark_span()
                    .start()
                    .expect("Span missing start")
                    .column();
                self.state = if self.node.mark_span().end().is_some() {
                    SpannedDeserializerState::SendEndSource
                } else {
                    SpannedDeserializerState::SendValue
                };
                seed.deserialize(v.into_deserializer())
            }
            SpannedDeserializerState::SendEndSource => {
                let v = self
                    .node
                    .mark_span()
                    .end()
                    .expect("Span missing end")
                    .source();
                self.state = SpannedDeserializerState::SendEndLine;
                seed.deserialize(v.into_deserializer())
            }
            SpannedDeserializerState::SendEndLine => {
                let v = self
                    .node
                    .mark_span()
                    .end()
                    .expect("Span missing end")
                    .line();
                self.state = SpannedDeserializerState::SendEndColumn;
                seed.deserialize(v.into_deserializer())
            }
            SpannedDeserializerState::SendEndColumn => {
                let v = self
                    .node
                    .mark_span()
                    .end()
                    .expect("Span missing end")
                    .column();
                self.state = SpannedDeserializerState::SendValue;
                seed.deserialize(v.into_deserializer())
            }
            SpannedDeserializerState::SendValue => {
                self.state = SpannedDeserializerState::Done;
                seed.deserialize(self.node.into_deserializer())
            }
            SpannedDeserializerState::Done => panic!("next_value_seed called before next_key_seed"),
        }
    }
}

// -------------------------------------------------------------------------------

impl<'de> IntoDeserializer<'de, Error> for &'de MarkedScalarNode {
    type Deserializer = MarkedScalarNodeDeserializer<'de>;
    fn into_deserializer(self) -> MarkedScalarNodeDeserializer<'de> {
        MarkedScalarNodeDeserializer { node: self }
    }
}

/// Deserializer for scalar nodes
pub struct MarkedScalarNodeDeserializer<'node> {
    node: &'node MarkedScalarNode,
}

macro_rules! scalar_fromstr {
    () => {
        scalar_fromstr!(deserialize_u8 visit_u8 u8);
        scalar_fromstr!(deserialize_u16 visit_u16 u16);
        scalar_fromstr!(deserialize_u32 visit_u32 u32);
        scalar_fromstr!(deserialize_u64 visit_u64 u64);
        scalar_fromstr!(deserialize_u128 visit_u128 u128);
        scalar_fromstr!(deserialize_i8 visit_i8 i8);
        scalar_fromstr!(deserialize_i16 visit_i16 i16);
        scalar_fromstr!(deserialize_i32 visit_i32 i32);
        scalar_fromstr!(deserialize_i64 visit_i64 i64);
        scalar_fromstr!(deserialize_i128 visit_i128 i128);
        scalar_fromstr!(deserialize_f32 visit_f32 f32);
        scalar_fromstr!(deserialize_f64 visit_f64 f64);
    };

    ($meth:ident $visit:ident $ty:ty) => {
        fn $meth<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let value: $ty = self.node.as_str().parse().addspans(*self.node.span())?;
            visitor.$visit(value)
        }
    };
}

impl<'de> Deserializer<'de> for MarkedScalarNodeDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.node
            .deref()
            .into_deserializer()
            .deserialize_any(visitor)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(
            self.node
                .as_bool()
                .ok_or(Error::NotBoolean(*self.node.span()))?,
        )
    }

    scalar_fromstr!();

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if name == SPANNED_TYPE && fields == SPANNED_FIELDS {
            return visitor.visit_map(SpannedDeserializer::new(self.node));
        }

        self.deserialize_any(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // Since we're here, there is no none, so visit as a some
        visitor.visit_some(self)
    }

    forward_to_deserialize_any! [
        char str string bytes byte_buf
        unit unit_struct newtype_struct seq tuple tuple_struct map
        enum identifier ignored_any
    ];
}

// -------------------------------------------------------------------------------

type MappingValueSeq<'de> = linked_hash_map::Iter<'de, MarkedScalarNode, Node>;
struct MappingAccess<'de> {
    items: Peekable<MappingValueSeq<'de>>,
}

impl<'de> MappingAccess<'de> {
    fn new(items: MappingValueSeq<'de>) -> Self {
        Self {
            items: items.peekable(),
        }
    }
}

impl<'de> MapAccess<'de> for MappingAccess<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        if let Some(next_key) = self.items.peek().map(|(k, _v)| k) {
            seed.deserialize(next_key.into_deserializer()).map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        seed.deserialize(
            self.items
                .next()
                .expect("next_value_seed called before next_key_seed")
                .1
                .into_deserializer(),
        )
    }
}

// -------------------------------------------------------------------------------

impl<'de> IntoDeserializer<'de, Error> for &'de MarkedMappingNode {
    type Deserializer = MarkedMappingNodeDeserializer<'de>;

    fn into_deserializer(self) -> Self::Deserializer {
        MarkedMappingNodeDeserializer { node: self }
    }
}

/// Deserializer for mapping nodes
pub struct MarkedMappingNodeDeserializer<'de> {
    node: &'de MarkedMappingNode,
}

impl<'de> Deserializer<'de> for MarkedMappingNodeDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(MappingAccess::new(self.node.iter()))
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if name == SPANNED_TYPE && fields == SPANNED_FIELDS {
            return visitor.visit_map(SpannedDeserializer::new(self.node));
        }

        self.deserialize_any(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // Since we're here, there is no none, so visit as a some
        visitor.visit_some(self)
    }

    forward_to_deserialize_any! [
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        unit unit_struct newtype_struct seq tuple tuple_struct
        map enum identifier ignored_any
    ];
}

// -------------------------------------------------------------------------------

struct SequenceAccess<'de> {
    items: &'de [Node],
    pos: usize,
}

impl<'de> SequenceAccess<'de> {
    fn new(items: &'de [Node]) -> Self {
        Self { items, pos: 0 }
    }
}

impl<'de> SeqAccess<'de> for SequenceAccess<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        if self.pos == self.items.len() {
            return Ok(None);
        }
        let pos = self.pos;
        self.pos += 1;

        seed.deserialize(self.items[pos].into_deserializer())
            .map(Some)
    }
}

// -------------------------------------------------------------------------------

impl<'de> IntoDeserializer<'de, Error> for &'de MarkedSequenceNode {
    type Deserializer = MarkedSequenceNodeDeserializer<'de>;

    fn into_deserializer(self) -> Self::Deserializer {
        MarkedSequenceNodeDeserializer { node: self }
    }
}

/// Deserializer for sequence nodes
pub struct MarkedSequenceNodeDeserializer<'de> {
    node: &'de MarkedSequenceNode,
}

impl<'de> Deserializer<'de> for MarkedSequenceNodeDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(SequenceAccess::new(self.node.as_slice()))
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if name == SPANNED_TYPE && fields == SPANNED_FIELDS {
            return visitor.visit_map(SpannedDeserializer::new(self.node));
        }

        self.deserialize_any(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // Since we're here, there is no none, so visit as a some
        visitor.visit_some(self)
    }

    forward_to_deserialize_any! [
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        unit unit_struct newtype_struct seq tuple tuple_struct
        map enum identifier ignored_any
    ];
}

// -------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    const TEST_DOC: &str = r#"hello: world
some: [ value, or, other ]
says: { grow: nothing, or: die }
numbers: [ 1, 2, 3, 500 ]
success: true
failure: False
shouting: TRUE
"#;

    #[test]
    #[allow(dead_code)]
    fn basic_deserialize() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            hello: String,
            some: Vec<String>,
            says: HashMap<String, String>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let doc: TestDoc = from_node(&node).unwrap();
        println!("{doc:#?}");
    }

    #[test]
    #[allow(dead_code)]
    fn basic_deserialize_spanned_scalars() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            hello: Spanned<String>,
            some: Vec<Spanned<String>>,
            says: HashMap<Spanned<String>, Spanned<String>>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let doc: TestDoc = from_node(&node).unwrap();
        println!("{doc:#?}");
    }

    #[test]
    #[allow(dead_code)]
    fn basic_deserialize_spanned_everything() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            hello: Spanned<String>,
            some: Spanned<Vec<Spanned<String>>>,
            says: Spanned<HashMap<Spanned<String>, Spanned<String>>>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let doc: Spanned<TestDoc> = from_node(&node).unwrap();
        println!("{doc:#?}");
    }

    #[test]
    #[allow(dead_code)]
    fn basic_deserialize_numbers() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            numbers: Vec<u16>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let doc: Spanned<TestDoc> = from_node(&node).unwrap();
        println!("{doc:#?}");
    }

    #[test]
    #[allow(dead_code)]
    #[cfg(not(feature = "serde-path"))]
    fn basic_deserialize_bad_numbers() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            numbers: Vec<u8>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let err = from_node::<TestDoc>(&node).err().unwrap();
        match err {
            Error::IntegerParseFailure(_e, s) => {
                let start = s.start().unwrap();
                assert_eq!(start.source(), 0);
                assert_eq!(start.line(), 4);
                assert_eq!(start.column(), 21);
            }
            _ => panic!("Unexpected error"),
        }
    }

    #[test]
    #[allow(dead_code)]
    #[cfg(feature = "serde-path")]
    fn basic_deserialize_bad_numbers() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            numbers: Vec<u8>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let err = from_node::<TestDoc>(&node).err().unwrap();
        assert_eq!(err.path().to_string(), "numbers[3]");
        let err = err.into_inner();
        match err {
            Error::IntegerParseFailure(_e, s) => {
                let start = s.start().unwrap();
                assert_eq!(start.source(), 0);
                assert_eq!(start.line(), 4);
                assert_eq!(start.column(), 21);
            }
            _ => panic!("Unexpected error"),
        }
    }

    #[test]
    #[allow(dead_code)]
    fn basic_deserialize_spanned_numbers() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            numbers: Vec<Spanned<i128>>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let doc: Spanned<TestDoc> = from_node(&node).unwrap();
        println!("{doc:#?}");
    }

    #[test]
    #[allow(dead_code)]
    fn basic_deserialize_bools() {
        #[derive(Deserialize, Debug)]
        struct TestDoc {
            success: bool,
            failure: Spanned<bool>,
            shouting: Spanned<bool>,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let doc: Spanned<TestDoc> = from_node(&node).unwrap();
        println!("{doc:#?}");
    }

    #[test]
    #[allow(dead_code)]
    fn disallowed_keys() {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct TestDoc {
            success: bool,
        }
        let node = crate::parse_yaml(0, TEST_DOC).unwrap();
        let err = from_node::<TestDoc>(&node).err().unwrap();
        #[cfg(feature = "serde-path")]
        let err = {
            assert_eq!(err.path().to_string(), "hello");
            let err = err.into_inner();
            let mark = err.start_mark().unwrap();
            assert_eq!(mark.source(), 0);
            assert_eq!(mark.line(), 1);
            assert_eq!(mark.column(), 1);
            err
        };
        assert!(matches!(err, Error::UnknownFieldError(_, _, _)));
    }
}
