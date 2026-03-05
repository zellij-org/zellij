use std::fmt::Display;
use thiserror::*;

/// The termwiz Error type encapsulates a range of internal
/// errors in an opaque manner.  You can use the `source`
/// method to reach the underlying errors if
/// necessary, but it is not expected that most code will
/// need to do so.  Please file an issue if you've got a
/// usecase for this!
#[derive(Error, Debug)]
#[error(transparent)]
pub struct Error(pub(crate) InternalError);

/// A Result whose error type is a termwiz Error
pub type Result<T> = std::result::Result<T, Error>;

impl<E> From<E> for Error
where
    E: Into<InternalError>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

/// This enum encapsulates the various errors that can be
/// mapped into the termwiz Error type.
/// The intent is that this is effectively private to termwiz
/// itself, but since Rust doesn't allow enums with private
/// variants, we're dancing around with a newtype of an enum
/// and hiding it from the docs.
#[derive(Error, Debug)]
#[non_exhaustive]
#[doc(hidden)]
pub enum InternalError {
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Regex(#[from] fancy_regex::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),

    #[error(transparent)]
    Base64(#[from] base64::DecodeError),

    #[error(transparent)]
    ParseFloat(#[from] std::num::ParseFloatError),

    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),

    #[error(transparent)]
    FloatIsNan(#[from] ordered_float::FloatIsNan),

    #[error("{0}")]
    StringErr(#[from] StringWrap),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    Terminfo(#[from] terminfo::Error),

    #[error(transparent)]
    FileDescriptor(#[from] filedescriptor::Error),

    #[error(transparent)]
    BlobLease(#[from] wezterm_blob_leases::Error),

    #[cfg(feature = "use_image")]
    #[error(transparent)]
    ImageError(#[from] image::ImageError),

    #[error("{}", .context)]
    Context {
        context: String,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

impl From<String> for InternalError {
    fn from(s: String) -> Self {
        InternalError::StringErr(StringWrap(s))
    }
}

#[derive(Error, Debug)]
#[doc(hidden)]
#[error("{0}")]
pub struct StringWrap(pub String);

#[macro_export]
macro_rules! vendored_termwiz_format_err {
    ($msg:literal $(,)?) => {
        return $crate::vendored::termwiz::error::Error::from($crate::vendored::termwiz::error::StringWrap($msg.to_string()))
    };
    ($err:expr $(,)?) => {
        return $crate::vendored::termwiz::error::Error::from($crate::vendored::termwiz::error::StringWrap(format!($err)))
    };
    ($fmt:expr, $($arg:tt)*) => {
        return $crate::vendored::termwiz::error::Error::from($crate::vendored::termwiz::error::StringWrap(format!($fmt, $($arg)*)))
    };
}

#[macro_export]
macro_rules! vendored_termwiz_bail {
    ($msg:literal $(,)?) => {
        return Err($crate::vendored::termwiz::error::StringWrap($msg.to_string()).into())
    };
    ($err:expr $(,)?) => {
        return Err($crate::vendored::termwiz::error::StringWrap(format!($err)).into())
    };
    ($fmt:expr, $($arg:tt)*) => {
        return Err($crate::vendored::termwiz::error::StringWrap(format!($fmt, $($arg)*)).into())
    };
}

#[macro_export]
macro_rules! vendored_termwiz_ensure {
    ($cond:expr, $msg:literal $(,)?) => {
        if !$cond {
            return Err($crate::vendored::termwiz::error::StringWrap(format!($msg)).into());
        }
    };
    ($cond:expr, $err:expr $(,)?) => {
        if !$cond {
            return Err($crate::vendored::termwiz::error::StringWrap(format!($err)).into());
        }
    };
    ($cond:expr, $fmt:expr, $($arg:tt)*) => {
        if !$cond {
            return Err($crate::vendored::termwiz::error::StringWrap(format!($fmt, $($arg)*)).into());
        }
    };
}

/// This trait allows extending the Result type so that it can create a
/// `termwiz::Error` that wraps an underlying other error and provide
/// additional context on that error.
pub trait Context<T, E> {
    /// Wrap the error value with additional context.
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display + Send + Sync + 'static;

    /// Wrap the error value with additional context that is evaluated lazily
    /// only once an error does occur.
    fn with_context<C, F>(self, f: F) -> Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T, E> Context<T, E> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display + Send + Sync + 'static,
    {
        self.map_err(|error| {
            Error(InternalError::Context {
                context: context.to_string(),
                source: Box::new(error),
            })
        })
    }

    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|error| {
            Error(InternalError::Context {
                context: context().to_string(),
                source: Box::new(error),
            })
        })
    }
}
