use std::fmt;

use bytes::Bytes;

#[derive(Debug, Clone, Copy)]
pub enum Code {
    Ok = 0,
    Cancelled = 1,
    Unknown = 2,
    InvalidArgument = 3,
    DeadlineExceeded = 4,
    NotFound = 5,
    AlreadyExist = 6,
    PermissionDenied = 7,
    ResourceExhausted = 8,
    FailedPrecondition = 9,
    Aborted = 10,
    OutOfRange = 11,
    Unimplemented = 12,
    Internal = 13,
    Unavailable = 14,
    DataLoss = 15,
    Unauthenticated = 16,
}

#[derive(Debug)]
pub struct Status {
    pub code: Code,
    pub message: String,
    detail: Bytes,
    cause: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl Status {
    pub fn new(code: Code, message: impl ToString) -> Self {
        Status {
            code,
            message: message.to_string(),
            detail: Bytes::new(),
            cause: None,
        }
    }

    pub fn with_detail(mut self, detail: Bytes) -> Self {
        self.detail = detail;
        self
    }

    pub fn with_cause<E: std::error::Error + Send + Sync + 'static>(mut self, cause: E) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Status code: {:?}, message: {:?}, detail: {:?}, cause: {:?}",
            self.code, self.message, self.detail, self.cause
        )
    }
}

impl std::error::Error for Status {}

impl From<hyper::Error> for Status {
    fn from(err: hyper::Error) -> Self {
        Status::new(Code::Internal, "http2 failed").with_cause(err)
    }
}

impl From<prost::EncodeError> for Status {
    fn from(err: prost::EncodeError) -> Self {
        Status::new(Code::Internal, "prost encode failed").with_cause(err)
    }
}

impl From<prost::DecodeError> for Status {
    fn from(err: prost::DecodeError) -> Self {
        Status::new(Code::Internal, "prost decode failed").with_cause(err)
    }
}
