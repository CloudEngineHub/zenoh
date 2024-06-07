//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! [Selector](https://github.com/eclipse-zenoh/roadmap/tree/main/rfcs/ALL/Selectors) to issue queries
use std::{
    borrow::Cow,
    collections::HashMap,
    convert::TryFrom,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use zenoh_protocol::core::{
    key_expr::{keyexpr, OwnedKeyExpr},
    Properties,
};
#[cfg(feature = "unstable")]
use zenoh_result::ZResult;
#[cfg(feature = "unstable")]
use zenoh_util::time_range::TimeRange;

use super::{key_expr::KeyExpr, queryable::Query};

/// A selector is the combination of a [Key Expression](crate::prelude::KeyExpr), which defines the
/// set of keys that are relevant to an operation, and a set of parameters
/// with a few intendend uses:
/// - specifying arguments to a queryable, allowing the passing of Remote Procedure Call parameters
/// - filtering by value,
/// - filtering by metadata, such as the timestamp of a value,
/// - specifying arguments to zenoh when using the REST API.
///
/// When in string form, selectors look a lot like a URI, with similar semantics:
/// - the `key_expr` before the first `?` must be a valid key expression.
/// - the `parameters` after the first `?` should be encoded like the query section of a URL:
///     - parameters are separated by `&`,
///     - the parameter name and value are separated by the first `=`,
///     - in the absence of `=`, the parameter value is considered to be the empty string,
///     - both name and value should use percent-encoding to escape characters,
///     - defining a value for the same parameter name twice is considered undefined behavior,
///       with the encouraged behaviour being to reject operations when a duplicate parameter is detected.
///
/// Zenoh intends to standardize the usage of a set of parameter names. To avoid conflicting with RPC parameters,
/// the Zenoh team has settled on reserving the set of parameter names that start with non-alphanumeric characters.
///
/// The full specification for selectors is available [here](https://github.com/eclipse-zenoh/roadmap/tree/main/rfcs/ALL/Selectors),
/// it includes standardized parameters.
///
/// Queryable implementers are encouraged to prefer these standardized parameter names when implementing their
/// associated features, and to prefix their own parameter names to avoid having conflicting parameter names with other
/// queryables.
///
/// Here are the currently standardized parameters for Zenoh (check the specification page for the exhaustive list):
/// - **`[unstable`** `_time`: used to express interest in only values dated within a certain time range, values for
///   this parameter must be readable by the [Zenoh Time DSL](zenoh_util::time_range::TimeRange) for the value to be considered valid.
/// - **`[unstable]`** `_anyke`: used in queries to express interest in replies coming from any key expression. By default, only replies
///   whose key expression match query's key expression are accepted. `_anyke` disables the query-reply key expression matching check.
///
/// The `Selector` type by itself is just a wrapper for tuple `(KeyExpr, Parameters)`. The only purpose of the separate type is to provide
/// conversion to and from string of the form `key_expr?parameters` using the 'From' and `TryFrom` trait.
#[derive(Clone, PartialEq, Eq)]
pub struct Selector<'a>(pub Cow<'a, KeyExpr<'a>>, pub Cow<'a, Parameters<'a>>);

#[zenoh_macros::unstable]
pub const TIME_RANGE_KEY: &str = "_time";

pub(crate) const _REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = "_anyke";
#[zenoh_macros::unstable]
pub const REPLY_KEY_EXPR_ANY_SEL_PARAM: &str = _REPLY_KEY_EXPR_ANY_SEL_PARAM;

/// Zenoh selector parameters
///
/// Most methods will return an Error if duplicates of a same parameter are found, to avoid HTTP Parameter Pollution like vulnerabilities.
#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Parameters<'a>(Properties<'a>);

impl<'a> Deref for Parameters<'a> {
    type Target = Properties<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for Parameters<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Display for Parameters<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for Parameters<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl<'a, T> From<T> for Parameters<'a>
where
    T: Into<Properties<'a>>,
{
    fn from(value: T) -> Self {
        Parameters(value.into())
    }
}

impl<'s> From<&'s Parameters<'s>> for HashMap<&'s str, &'s str> {
    fn from(props: &'s Parameters<'s>) -> Self {
        HashMap::from(&props.0)
    }
}

impl From<&Parameters<'_>> for HashMap<String, String> {
    fn from(props: &Parameters) -> Self {
        HashMap::from(&props.0)
    }
}

impl From<Parameters<'_>> for HashMap<String, String> {
    fn from(props: Parameters) -> Self {
        HashMap::from(props.0)
    }
}

impl Parameters<'_> {
    /// Create an owned version of these parameters with `'static` lifetime.
    pub fn into_owned(self) -> Parameters<'static> {
        Parameters(self.0.into_owned())
    }

    #[zenoh_macros::unstable]
    /// Sets the time range targeted by the selector.
    pub fn set_time_range<T: Into<Option<TimeRange>>>(&mut self, time_range: T) {
        let mut time_range: Option<TimeRange> = time_range.into();
        match time_range.take() {
            Some(tr) => self.0.insert(TIME_RANGE_KEY, format!("{}", tr)),
            None => self.0.remove(TIME_RANGE_KEY),
        };
    }

    #[zenoh_macros::unstable]
    /// Extracts the standardized `_time` argument from the selector parameters.
    ///
    /// The default implementation still causes a complete pass through the selector parameters to ensure that there are no duplicates of the `_time` key.
    pub fn time_range(&self) -> ZResult<Option<TimeRange>> {
        match self.0.get(TIME_RANGE_KEY) {
            Some(tr) => Ok(Some(tr.parse()?)),
            None => Ok(None),
        }
    }
}

impl std::fmt::Debug for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "sel\"{self}\"")
    }
}

impl std::fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Self(key_expr, parameters) = self;
        write!(f, "{}", key_expr)?;
        if !parameters.is_empty() {
            write!(f, "?{}", parameters.as_str())?;
        }
        Ok(())
    }
}

impl<'a, K, P> From<(K, P)> for Selector<'a>
where
    K: Into<KeyExpr<'a>>,
    P: Into<Parameters<'a>>,
{
    fn from(value: (K, P)) -> Self {
        Self(Cow::Owned(value.0.into()), Cow::Owned(value.1.into()))
    }
}

impl<'a> From<(&'a KeyExpr<'a>, &'a Parameters<'a>)> for Selector<'a> {
    fn from(value: (&'a KeyExpr<'a>, &'a Parameters<'a>)) -> Self {
        Self(Cow::Borrowed(value.0), Cow::Borrowed(value.1))
    }
}

impl<'a> From<Selector<'a>> for (KeyExpr<'a>, Parameters<'a>) {
    fn from(value: Selector<'a>) -> Self {
        (value.0.into_owned(), value.1.into_owned())
    }
}

impl<'a> From<&'a Selector<'a>> for (&'a KeyExpr<'a>, &'a Parameters<'a>) {
    fn from(value: &'a Selector<'a>) -> Self {
        (value.0.as_ref(), value.1.as_ref())
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl TryFrom<String> for Selector<'_> {
    type Error = zenoh_result::Error;
    fn try_from(mut s: String) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let parameters = s[qmark_position + 1..].to_owned();
                s.truncate(qmark_position);
                Ok(Self(
                    Cow::Owned(KeyExpr::try_from(s)?),
                    Cow::Owned(parameters.into()),
                ))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
    }
}

impl<'a> TryFrom<&'a str> for Selector<'a> {
    type Error = zenoh_result::Error;
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let params = &s[qmark_position + 1..];
                Ok(Self(
                    Cow::Owned(KeyExpr::try_from(&s[..qmark_position])?),
                    Cow::Owned(params.into()),
                ))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
    }
}
impl FromStr for Selector<'static> {
    type Err = zenoh_result::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key_expr: KeyExpr = s.try_into()?;
        Ok(Selector(
            Cow::Owned(key_expr.into_owned()),
            Cow::Owned(Parameters::default()),
        ))
    }
}

impl<'a> TryFrom<&'a String> for Selector<'a> {
    type Error = zenoh_result::Error;
    fn try_from(s: &'a String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl<'a> From<&'a Query> for Selector<'a> {
    fn from(q: &'a Query) -> Self {
        Self(
            Cow::Borrowed(&q.inner.key_expr),
            Cow::Borrowed(&q.inner.parameters),
        )
    }
}

impl<'a> From<&'a KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: &'a KeyExpr<'a>) -> Self {
        Self(
            Cow::Borrowed(key_selector),
            Cow::Owned(Parameters::default()),
        )
    }
}

impl<'a> From<&'a keyexpr> for Selector<'a> {
    fn from(key_selector: &'a keyexpr) -> Self {
        Self(
            Cow::Owned(key_selector.into()),
            Cow::Owned(Parameters::default()),
        )
    }
}

impl<'a> From<&'a OwnedKeyExpr> for Selector<'a> {
    fn from(key_selector: &'a OwnedKeyExpr) -> Self {
        Self(
            Cow::Owned(key_selector.into()),
            Cow::Owned(Parameters::default()),
        )
    }
}

impl From<OwnedKeyExpr> for Selector<'static> {
    fn from(key_selector: OwnedKeyExpr) -> Self {
        Self(
            Cow::Owned(key_selector.into()),
            Cow::Owned(Parameters::default()),
        )
    }
}

impl<'a> From<KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: KeyExpr<'a>) -> Self {
        Self(Cow::Owned(key_selector), Cow::Owned(Parameters::default()))
    }
}

#[test]
fn selector_accessors() {
    use crate::api::selector::_REPLY_KEY_EXPR_ANY_SEL_PARAM as ANYKE;

    for selector in [
        "hello/there?_timetrick",
        "hello/there?_timetrick;_time",
        "hello/there?_timetrick;_time;_filter",
        "hello/there?_timetrick;_time=[..]",
        "hello/there?_timetrick;_time=[..];_filter",
    ] {
        let Selector(key_expr, parameters) = selector.try_into().unwrap();
        let mut parameters = parameters.into_owned();

        assert_eq!(key_expr.as_str(), "hello/there");

        println!("Parameters start: {}", parameters);
        for i in parameters.iter() {
            println!("\t{:?}", i);
        }

        assert_eq!(parameters.get("_timetrick").unwrap(), "");

        let time_range = "[now(-2s)..now(2s)]";
        zcondfeat!(
            "unstable",
            {
                let time_range = time_range.parse().unwrap();
                parameters.set_time_range(time_range);
                assert_eq!(parameters.time_range().unwrap().unwrap(), time_range);
            },
            {
                parameters.insert(TIME_RANGE_KEY, time_range);
            }
        );
        assert_eq!(parameters.get(TIME_RANGE_KEY).unwrap(), time_range);

        let hm: HashMap<&str, &str> = HashMap::from(&parameters);
        assert!(hm.contains_key(TIME_RANGE_KEY));

        parameters.insert("_filter", "");
        assert_eq!(parameters.get("_filter").unwrap(), "");

        let hm: HashMap<String, String> = HashMap::from(&parameters);
        assert!(hm.contains_key(TIME_RANGE_KEY));

        parameters.extend_from_iter(hm.iter());
        assert_eq!(parameters.get("_filter").unwrap(), "");

        parameters.insert(ANYKE, "");

        println!("Parameters end: {}", parameters);
        for i in parameters.iter() {
            println!("\t{:?}", i);
        }

        assert_eq!(
            HashMap::<String, String>::from(&parameters),
            HashMap::<String, String>::from(Parameters::from(
                "_anyke;_filter;_time=[now(-2s)..now(2s)];_timetrick"
            ))
        );
    }
}
