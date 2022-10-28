use std::{fmt, str::FromStr};

use bytes::Bytes;
use hyper::http;

use crate::grpc::headers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
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

impl Code {
    pub fn from_bytes(b: &[u8]) -> Result<Self, Status> {
        match b.len() {
            1 if b[0].is_ascii_digit() => Code::try_from(b[0] - b'0'),
            2 if b[0] == b'1' && b[1].is_ascii_digit() => {
                let code = (b[0] - b'0') * 10;
                let code = code + b[1] - b'0';
                Code::try_from(code)
            }
            _ => Err(Status::new(
                Code::Internal,
                format!("invalid grpc-status({:?})", b),
            )),
        }
    }
}

impl TryFrom<u8> for Code {
    type Error = Status;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let code = match value {
            0 => Code::Ok,
            1 => Code::Cancelled,
            2 => Code::Unknown,
            3 => Code::InvalidArgument,
            4 => Code::DeadlineExceeded,
            5 => Code::NotFound,
            6 => Code::AlreadyExist,
            7 => Code::PermissionDenied,
            8 => Code::ResourceExhausted,
            9 => Code::FailedPrecondition,
            10 => Code::Aborted,
            11 => Code::OutOfRange,
            12 => Code::Unimplemented,
            13 => Code::Internal,
            14 => Code::Unavailable,
            15 => Code::DataLoss,
            16 => Self::Unauthenticated,
            _ => {
                return Err(Status::new(
                    Code::Internal,
                    format!("invalid grpc-status({:?})", value),
                ))
            }
        };

        Ok(code)
    }
}

impl FromStr for Code {
    type Err = Status;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let code = s
            .parse::<u8>()
            .map_err(|err| Status::internal("parse grpc-status failed").with_cause(err))?;

        Code::try_from(code)
    }
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

    pub fn is_ok(&self) -> bool {
        self.code == Code::Ok
    }

    pub fn from_http_status(status_code: hyper::StatusCode) -> Status {
        let code = match status_code {
            // Borrowed from https://github.com/grpc/grpc/blob/master/doc/http-grpc-status-mapping.md
            http::StatusCode::BAD_REQUEST => Code::Internal,
            http::StatusCode::UNAUTHORIZED => Code::Unauthenticated,
            http::StatusCode::FORBIDDEN => Code::PermissionDenied,
            http::StatusCode::NOT_FOUND => Code::Unimplemented,
            http::StatusCode::TOO_MANY_REQUESTS
            | http::StatusCode::BAD_GATEWAY
            | http::StatusCode::SERVICE_UNAVAILABLE
            | http::StatusCode::GATEWAY_TIMEOUT => Code::Unavailable,
            http::StatusCode::OK => Code::Ok,
            _ => Code::Unknown,
        };

        let msg = format!(
            "grpc-status header missing, mapped from HTTP status code {}",
            status_code.as_u16(),
        );

        Status::new(code, msg)
    }

    pub fn from_header_map(headers: &hyper::HeaderMap) -> Result<Status, Status> {
        let code = headers
            .get(headers::GRPC_STATUS)
            .ok_or_else(|| Status::internal("grpc-status not found"))?;
        let code = Code::from_bytes(code.as_bytes())?;

        // message can be empty
        let message = headers
            .get(headers::GRPC_MESSAGE)
            .map(|s| String::from_utf8_lossy(s.as_bytes()).to_string())
            .unwrap_or_default();

        let detail = headers
            .get(headers::GRPC_STATUS_DETAIL_BIN)
            .map(|s| Bytes::from(s.as_bytes().to_vec()))
            .unwrap_or_default();

        Ok(Status::new(code, message).with_detail(detail))
    }

    pub fn internal(message: impl ToString) -> Self {
        Self::new(Code::Internal, message)
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
        Status::internal("http2 failed").with_cause(err)
    }
}

impl From<prost::EncodeError> for Status {
    fn from(err: prost::EncodeError) -> Self {
        Status::internal("prost encode failed").with_cause(err)
    }
}

impl From<prost::DecodeError> for Status {
    fn from(err: prost::DecodeError) -> Self {
        Status::internal("prost decode failed").with_cause(err)
    }
}
