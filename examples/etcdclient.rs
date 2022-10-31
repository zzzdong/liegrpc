mod mvccpb {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct KeyValue {
        /// key is the key in bytes. An empty key is not allowed.
        #[prost(bytes = "vec", tag = "1")]
        pub key: ::prost::alloc::vec::Vec<u8>,
        /// create_revision is the revision of last creation on this key.
        #[prost(int64, tag = "2")]
        pub create_revision: i64,
        /// mod_revision is the revision of last modification on this key.
        #[prost(int64, tag = "3")]
        pub mod_revision: i64,
        /// version is the version of the key. A deletion resets
        /// the version to zero and any modification of the key
        /// increases its version.
        #[prost(int64, tag = "4")]
        pub version: i64,
        /// value is the value held by the key, in bytes.
        #[prost(bytes = "vec", tag = "5")]
        pub value: ::prost::alloc::vec::Vec<u8>,
        /// lease is the ID of the lease that attached to key.
        /// When the attached lease expires, the key will be deleted.
        /// If lease is 0, then no lease is attached to the key.
        #[prost(int64, tag = "6")]
        pub lease: i64,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Event {
        /// type is the kind of event. If type is a PUT, it indicates
        /// new data has been stored to the key. If type is a DELETE,
        /// it indicates the key was deleted.
        #[prost(enumeration = "event::EventType", tag = "1")]
        pub r#type: i32,
        /// kv holds the KeyValue for the event.
        /// A PUT event contains current kv pair.
        /// A PUT event with kv.Version=1 indicates the creation of a key.
        /// A DELETE/EXPIRE event contains the deleted key with
        /// its modification revision set to the revision of deletion.
        #[prost(message, optional, tag = "2")]
        pub kv: ::core::option::Option<KeyValue>,
        /// prev_kv holds the key-value pair before the event happens.
        #[prost(message, optional, tag = "3")]
        pub prev_kv: ::core::option::Option<KeyValue>,
    }
    /// Nested message and enum types in `Event`.
    pub mod event {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum EventType {
            Put = 0,
            Delete = 1,
        }
        impl EventType {
            /// String value of the enum field names used in the ProtoBuf definition.
            ///
            /// The values are not transformed in any way and thus are considered stable
            /// (if the ProtoBuf definition does not change) and safe for programmatic use.
            pub fn as_str_name(&self) -> &'static str {
                match self {
                    EventType::Put => "PUT",
                    EventType::Delete => "DELETE",
                }
            }
        }
    }
}

mod authpb {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct UserAddOptions {
        #[prost(bool, tag = "1")]
        pub no_password: bool,
    }
    /// User is a single entry in the bucket authUsers
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct User {
        #[prost(bytes = "vec", tag = "1")]
        pub name: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes = "vec", tag = "2")]
        pub password: ::prost::alloc::vec::Vec<u8>,
        #[prost(string, repeated, tag = "3")]
        pub roles: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
        #[prost(message, optional, tag = "4")]
        pub options: ::core::option::Option<UserAddOptions>,
    }
    /// Permission is a single entity
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Permission {
        #[prost(enumeration = "permission::Type", tag = "1")]
        pub perm_type: i32,
        #[prost(bytes = "vec", tag = "2")]
        pub key: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes = "vec", tag = "3")]
        pub range_end: ::prost::alloc::vec::Vec<u8>,
    }
    /// Nested message and enum types in `Permission`.
    pub mod permission {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum Type {
            Read = 0,
            Write = 1,
            Readwrite = 2,
        }
        impl Type {
            /// String value of the enum field names used in the ProtoBuf definition.
            ///
            /// The values are not transformed in any way and thus are considered stable
            /// (if the ProtoBuf definition does not change) and safe for programmatic use.
            pub fn as_str_name(&self) -> &'static str {
                match self {
                    Type::Read => "READ",
                    Type::Write => "WRITE",
                    Type::Readwrite => "READWRITE",
                }
            }
        }
    }
    /// Role is a single entry in the bucket authRoles
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Role {
        #[prost(bytes = "vec", tag = "1")]
        pub name: ::prost::alloc::vec::Vec<u8>,
        #[prost(message, repeated, tag = "2")]
        pub key_permission: ::prost::alloc::vec::Vec<Permission>,
    }
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResponseHeader {
    /// cluster_id is the ID of the cluster which sent the response.
    #[prost(uint64, tag = "1")]
    pub cluster_id: u64,
    /// member_id is the ID of the member which sent the response.
    #[prost(uint64, tag = "2")]
    pub member_id: u64,
    /// revision is the key-value store revision when the request was applied.
    /// For watch progress responses, the header.revision indicates progress. All future events
    /// recieved in this stream are guaranteed to have a higher revision number than the
    /// header.revision number.
    #[prost(int64, tag = "3")]
    pub revision: i64,
    /// raft_term is the raft term when the request was applied.
    #[prost(uint64, tag = "4")]
    pub raft_term: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RangeRequest {
    /// key is the first key for the range. If range_end is not given, the request only looks up key.
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    /// range_end is the upper bound on the requested range [key, range_end).
    /// If range_end is '\0', the range is all keys >= key.
    /// If range_end is key plus one (e.g., "aa"+1 == "ab", "a\xff"+1 == "b"),
    /// then the range request gets all keys prefixed with key.
    /// If both key and range_end are '\0', then the range request returns all keys.
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    /// limit is a limit on the number of keys returned for the request. When limit is set to 0,
    /// it is treated as no limit.
    #[prost(int64, tag = "3")]
    pub limit: i64,
    /// revision is the point-in-time of the key-value store to use for the range.
    /// If revision is less or equal to zero, the range is over the newest key-value store.
    /// If the revision has been compacted, ErrCompacted is returned as a response.
    #[prost(int64, tag = "4")]
    pub revision: i64,
    /// sort_order is the order for returned sorted results.
    #[prost(enumeration = "range_request::SortOrder", tag = "5")]
    pub sort_order: i32,
    /// sort_target is the key-value field to use for sorting.
    #[prost(enumeration = "range_request::SortTarget", tag = "6")]
    pub sort_target: i32,
    /// serializable sets the range request to use serializable member-local reads.
    /// Range requests are linearizable by default; linearizable requests have higher
    /// latency and lower throughput than serializable requests but reflect the current
    /// consensus of the cluster. For better performance, in exchange for possible stale reads,
    /// a serializable range request is served locally without needing to reach consensus
    /// with other nodes in the cluster.
    #[prost(bool, tag = "7")]
    pub serializable: bool,
    /// keys_only when set returns only the keys and not the values.
    #[prost(bool, tag = "8")]
    pub keys_only: bool,
    /// count_only when set returns only the count of the keys in the range.
    #[prost(bool, tag = "9")]
    pub count_only: bool,
    /// min_mod_revision is the lower bound for returned key mod revisions; all keys with
    /// lesser mod revisions will be filtered away.
    #[prost(int64, tag = "10")]
    pub min_mod_revision: i64,
    /// max_mod_revision is the upper bound for returned key mod revisions; all keys with
    /// greater mod revisions will be filtered away.
    #[prost(int64, tag = "11")]
    pub max_mod_revision: i64,
    /// min_create_revision is the lower bound for returned key create revisions; all keys with
    /// lesser create revisions will be filtered away.
    #[prost(int64, tag = "12")]
    pub min_create_revision: i64,
    /// max_create_revision is the upper bound for returned key create revisions; all keys with
    /// greater create revisions will be filtered away.
    #[prost(int64, tag = "13")]
    pub max_create_revision: i64,
}
/// Nested message and enum types in `RangeRequest`.
pub mod range_request {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum SortOrder {
        /// default, no sorting
        None = 0,
        /// lowest target value first
        Ascend = 1,
        /// highest target value first
        Descend = 2,
    }
    impl SortOrder {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                SortOrder::None => "NONE",
                SortOrder::Ascend => "ASCEND",
                SortOrder::Descend => "DESCEND",
            }
        }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum SortTarget {
        Key = 0,
        Version = 1,
        Create = 2,
        Mod = 3,
        Value = 4,
    }
    impl SortTarget {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                SortTarget::Key => "KEY",
                SortTarget::Version => "VERSION",
                SortTarget::Create => "CREATE",
                SortTarget::Mod => "MOD",
                SortTarget::Value => "VALUE",
            }
        }
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RangeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// kvs is the list of key-value pairs matched by the range request.
    /// kvs is empty when count is requested.
    #[prost(message, repeated, tag = "2")]
    pub kvs: ::prost::alloc::vec::Vec<mvccpb::KeyValue>,
    /// more indicates if there are more keys to return in the requested range.
    #[prost(bool, tag = "3")]
    pub more: bool,
    /// count is set to the number of keys within the range when requested.
    #[prost(int64, tag = "4")]
    pub count: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PutRequest {
    /// key is the key, in bytes, to put into the key-value store.
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    /// value is the value, in bytes, to associate with the key in the key-value store.
    #[prost(bytes = "vec", tag = "2")]
    pub value: ::prost::alloc::vec::Vec<u8>,
    /// lease is the lease ID to associate with the key in the key-value store. A lease
    /// value of 0 indicates no lease.
    #[prost(int64, tag = "3")]
    pub lease: i64,
    /// If prev_kv is set, etcd gets the previous key-value pair before changing it.
    /// The previous key-value pair will be returned in the put response.
    #[prost(bool, tag = "4")]
    pub prev_kv: bool,
    /// If ignore_value is set, etcd updates the key using its current value.
    /// Returns an error if the key does not exist.
    #[prost(bool, tag = "5")]
    pub ignore_value: bool,
    /// If ignore_lease is set, etcd updates the key using its current lease.
    /// Returns an error if the key does not exist.
    #[prost(bool, tag = "6")]
    pub ignore_lease: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PutResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// if prev_kv is set in the request, the previous key-value pair will be returned.
    #[prost(message, optional, tag = "2")]
    pub prev_kv: ::core::option::Option<mvccpb::KeyValue>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteRangeRequest {
    /// key is the first key to delete in the range.
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    /// range_end is the key following the last key to delete for the range [key, range_end).
    /// If range_end is not given, the range is defined to contain only the key argument.
    /// If range_end is one bit larger than the given key, then the range is all the keys
    /// with the prefix (the given key).
    /// If range_end is '\0', the range is all keys greater than or equal to the key argument.
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    /// If prev_kv is set, etcd gets the previous key-value pairs before deleting it.
    /// The previous key-value pairs will be returned in the delete response.
    #[prost(bool, tag = "3")]
    pub prev_kv: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteRangeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// deleted is the number of keys deleted by the delete range request.
    #[prost(int64, tag = "2")]
    pub deleted: i64,
    /// if prev_kv is set in the request, the previous key-value pairs will be returned.
    #[prost(message, repeated, tag = "3")]
    pub prev_kvs: ::prost::alloc::vec::Vec<mvccpb::KeyValue>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RequestOp {
    /// request is a union of request types accepted by a transaction.
    #[prost(oneof = "request_op::Request", tags = "1, 2, 3, 4")]
    pub request: ::core::option::Option<request_op::Request>,
}
/// Nested message and enum types in `RequestOp`.
pub mod request_op {
    /// request is a union of request types accepted by a transaction.
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Request {
        #[prost(message, tag = "1")]
        RequestRange(super::RangeRequest),
        #[prost(message, tag = "2")]
        RequestPut(super::PutRequest),
        #[prost(message, tag = "3")]
        RequestDeleteRange(super::DeleteRangeRequest),
        #[prost(message, tag = "4")]
        RequestTxn(super::TxnRequest),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResponseOp {
    /// response is a union of response types returned by a transaction.
    #[prost(oneof = "response_op::Response", tags = "1, 2, 3, 4")]
    pub response: ::core::option::Option<response_op::Response>,
}
/// Nested message and enum types in `ResponseOp`.
pub mod response_op {
    /// response is a union of response types returned by a transaction.
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Response {
        #[prost(message, tag = "1")]
        ResponseRange(super::RangeResponse),
        #[prost(message, tag = "2")]
        ResponsePut(super::PutResponse),
        #[prost(message, tag = "3")]
        ResponseDeleteRange(super::DeleteRangeResponse),
        #[prost(message, tag = "4")]
        ResponseTxn(super::TxnResponse),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Compare {
    /// result is logical comparison operation for this comparison.
    #[prost(enumeration = "compare::CompareResult", tag = "1")]
    pub result: i32,
    /// target is the key-value field to inspect for the comparison.
    #[prost(enumeration = "compare::CompareTarget", tag = "2")]
    pub target: i32,
    /// key is the subject key for the comparison operation.
    #[prost(bytes = "vec", tag = "3")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    /// range_end compares the given target to all keys in the range [key, range_end).
    /// See RangeRequest for more details on key ranges.
    ///
    /// TODO: fill out with most of the rest of RangeRequest fields when needed.
    #[prost(bytes = "vec", tag = "64")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    #[prost(oneof = "compare::TargetUnion", tags = "4, 5, 6, 7, 8")]
    pub target_union: ::core::option::Option<compare::TargetUnion>,
}
/// Nested message and enum types in `Compare`.
pub mod compare {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum CompareResult {
        Equal = 0,
        Greater = 1,
        Less = 2,
        NotEqual = 3,
    }
    impl CompareResult {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                CompareResult::Equal => "EQUAL",
                CompareResult::Greater => "GREATER",
                CompareResult::Less => "LESS",
                CompareResult::NotEqual => "NOT_EQUAL",
            }
        }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum CompareTarget {
        Version = 0,
        Create = 1,
        Mod = 2,
        Value = 3,
        Lease = 4,
    }
    impl CompareTarget {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                CompareTarget::Version => "VERSION",
                CompareTarget::Create => "CREATE",
                CompareTarget::Mod => "MOD",
                CompareTarget::Value => "VALUE",
                CompareTarget::Lease => "LEASE",
            }
        }
    }
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum TargetUnion {
        /// version is the version of the given key
        #[prost(int64, tag = "4")]
        Version(i64),
        /// create_revision is the creation revision of the given key
        #[prost(int64, tag = "5")]
        CreateRevision(i64),
        /// mod_revision is the last modified revision of the given key.
        #[prost(int64, tag = "6")]
        ModRevision(i64),
        /// value is the value of the given key, in bytes.
        #[prost(bytes, tag = "7")]
        Value(::prost::alloc::vec::Vec<u8>),
        /// lease is the lease id of the given key.
        ///
        /// leave room for more target_union field tags, jump to 64
        #[prost(int64, tag = "8")]
        Lease(i64),
    }
}
/// From google paxosdb paper:
/// Our implementation hinges around a powerful primitive which we call MultiOp. All other database
/// operations except for iteration are implemented as a single call to MultiOp. A MultiOp is applied atomically
/// and consists of three components:
/// 1. A list of tests called guard. Each test in guard checks a single entry in the database. It may check
/// for the absence or presence of a value, or compare with a given value. Two different tests in the guard
/// may apply to the same or different entries in the database. All tests in the guard are applied and
/// MultiOp returns the results. If all tests are true, MultiOp executes t op (see item 2 below), otherwise
/// it executes f op (see item 3 below).
/// 2. A list of database operations called t op. Each operation in the list is either an insert, delete, or
/// lookup operation, and applies to a single database entry. Two different operations in the list may apply
/// to the same or different entries in the database. These operations are executed
/// if guard evaluates to
/// true.
/// 3. A list of database operations called f op. Like t op, but executed if guard evaluates to false.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TxnRequest {
    /// compare is a list of predicates representing a conjunction of terms.
    /// If the comparisons succeed, then the success requests will be processed in order,
    /// and the response will contain their respective responses in order.
    /// If the comparisons fail, then the failure requests will be processed in order,
    /// and the response will contain their respective responses in order.
    #[prost(message, repeated, tag = "1")]
    pub compare: ::prost::alloc::vec::Vec<Compare>,
    /// success is a list of requests which will be applied when compare evaluates to true.
    #[prost(message, repeated, tag = "2")]
    pub success: ::prost::alloc::vec::Vec<RequestOp>,
    /// failure is a list of requests which will be applied when compare evaluates to false.
    #[prost(message, repeated, tag = "3")]
    pub failure: ::prost::alloc::vec::Vec<RequestOp>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TxnResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// succeeded is set to true if the compare evaluated to true or false otherwise.
    #[prost(bool, tag = "2")]
    pub succeeded: bool,
    /// responses is a list of responses corresponding to the results from applying
    /// success if succeeded is true or failure if succeeded is false.
    #[prost(message, repeated, tag = "3")]
    pub responses: ::prost::alloc::vec::Vec<ResponseOp>,
}
/// CompactionRequest compacts the key-value store up to a given revision. All superseded keys
/// with a revision less than the compaction revision will be removed.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CompactionRequest {
    /// revision is the key-value store revision for the compaction operation.
    #[prost(int64, tag = "1")]
    pub revision: i64,
    /// physical is set so the RPC will wait until the compaction is physically
    /// applied to the local database such that compacted entries are totally
    /// removed from the backend database.
    #[prost(bool, tag = "2")]
    pub physical: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CompactionResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HashRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HashKvRequest {
    /// revision is the key-value store revision for the hash operation.
    #[prost(int64, tag = "1")]
    pub revision: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HashKvResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// hash is the hash value computed from the responding member's MVCC keys up to a given revision.
    #[prost(uint32, tag = "2")]
    pub hash: u32,
    /// compact_revision is the compacted revision of key-value store when hash begins.
    #[prost(int64, tag = "3")]
    pub compact_revision: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HashResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// hash is the hash value computed from the responding member's KV's backend.
    #[prost(uint32, tag = "2")]
    pub hash: u32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SnapshotRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SnapshotResponse {
    /// header has the current key-value store information. The first header in the snapshot
    /// stream indicates the point in time of the snapshot.
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// remaining_bytes is the number of blob bytes to be sent after this message
    #[prost(uint64, tag = "2")]
    pub remaining_bytes: u64,
    /// blob contains the next chunk of the snapshot in the snapshot stream.
    #[prost(bytes = "vec", tag = "3")]
    pub blob: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WatchRequest {
    /// request_union is a request to either create a new watcher or cancel an existing watcher.
    #[prost(oneof = "watch_request::RequestUnion", tags = "1, 2, 3")]
    pub request_union: ::core::option::Option<watch_request::RequestUnion>,
}
/// Nested message and enum types in `WatchRequest`.
pub mod watch_request {
    /// request_union is a request to either create a new watcher or cancel an existing watcher.
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum RequestUnion {
        #[prost(message, tag = "1")]
        CreateRequest(super::WatchCreateRequest),
        #[prost(message, tag = "2")]
        CancelRequest(super::WatchCancelRequest),
        #[prost(message, tag = "3")]
        ProgressRequest(super::WatchProgressRequest),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WatchCreateRequest {
    /// key is the key to register for watching.
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    /// range_end is the end of the range [key, range_end) to watch. If range_end is not given,
    /// only the key argument is watched. If range_end is equal to '\0', all keys greater than
    /// or equal to the key argument are watched.
    /// If the range_end is one bit larger than the given key,
    /// then all keys with the prefix (the given key) will be watched.
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    /// start_revision is an optional revision to watch from (inclusive). No start_revision is "now".
    #[prost(int64, tag = "3")]
    pub start_revision: i64,
    /// progress_notify is set so that the etcd server will periodically send a WatchResponse with
    /// no events to the new watcher if there are no recent events. It is useful when clients
    /// wish to recover a disconnected watcher starting from a recent known revision.
    /// The etcd server may decide how often it will send notifications based on current load.
    #[prost(bool, tag = "4")]
    pub progress_notify: bool,
    /// filters filter the events at server side before it sends back to the watcher.
    #[prost(enumeration = "watch_create_request::FilterType", repeated, tag = "5")]
    pub filters: ::prost::alloc::vec::Vec<i32>,
    /// If prev_kv is set, created watcher gets the previous KV before the event happens.
    /// If the previous KV is already compacted, nothing will be returned.
    #[prost(bool, tag = "6")]
    pub prev_kv: bool,
    /// If watch_id is provided and non-zero, it will be assigned to this watcher.
    /// Since creating a watcher in etcd is not a synchronous operation,
    /// this can be used ensure that ordering is correct when creating multiple
    /// watchers on the same stream. Creating a watcher with an ID already in
    /// use on the stream will cause an error to be returned.
    #[prost(int64, tag = "7")]
    pub watch_id: i64,
    /// fragment enables splitting large revisions into multiple watch responses.
    #[prost(bool, tag = "8")]
    pub fragment: bool,
}
/// Nested message and enum types in `WatchCreateRequest`.
pub mod watch_create_request {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum FilterType {
        /// filter out put event.
        Noput = 0,
        /// filter out delete event.
        Nodelete = 1,
    }
    impl FilterType {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                FilterType::Noput => "NOPUT",
                FilterType::Nodelete => "NODELETE",
            }
        }
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WatchCancelRequest {
    /// watch_id is the watcher id to cancel so that no more events are transmitted.
    #[prost(int64, tag = "1")]
    pub watch_id: i64,
}
/// Requests the a watch stream progress status be sent in the watch response stream as soon as
/// possible.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WatchProgressRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WatchResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// watch_id is the ID of the watcher that corresponds to the response.
    #[prost(int64, tag = "2")]
    pub watch_id: i64,
    /// created is set to true if the response is for a create watch request.
    /// The client should record the watch_id and expect to receive events for
    /// the created watcher from the same stream.
    /// All events sent to the created watcher will attach with the same watch_id.
    #[prost(bool, tag = "3")]
    pub created: bool,
    /// canceled is set to true if the response is for a cancel watch request.
    /// No further events will be sent to the canceled watcher.
    #[prost(bool, tag = "4")]
    pub canceled: bool,
    /// compact_revision is set to the minimum index if a watcher tries to watch
    /// at a compacted index.
    ///
    /// This happens when creating a watcher at a compacted revision or the watcher cannot
    /// catch up with the progress of the key-value store.
    ///
    /// The client should treat the watcher as canceled and should not try to create any
    /// watcher with the same start_revision again.
    #[prost(int64, tag = "5")]
    pub compact_revision: i64,
    /// cancel_reason indicates the reason for canceling the watcher.
    #[prost(string, tag = "6")]
    pub cancel_reason: ::prost::alloc::string::String,
    /// framgment is true if large watch response was split over multiple responses.
    #[prost(bool, tag = "7")]
    pub fragment: bool,
    #[prost(message, repeated, tag = "11")]
    pub events: ::prost::alloc::vec::Vec<mvccpb::Event>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseGrantRequest {
    /// TTL is the advisory time-to-live in seconds. Expired lease will return -1.
    #[prost(int64, tag = "1")]
    pub ttl: i64,
    /// ID is the requested ID for the lease. If ID is set to 0, the lessor chooses an ID.
    #[prost(int64, tag = "2")]
    pub id: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseGrantResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// ID is the lease ID for the granted lease.
    #[prost(int64, tag = "2")]
    pub id: i64,
    /// TTL is the server chosen lease time-to-live in seconds.
    #[prost(int64, tag = "3")]
    pub ttl: i64,
    #[prost(string, tag = "4")]
    pub error: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseRevokeRequest {
    /// ID is the lease ID to revoke. When the ID is revoked, all associated keys will be deleted.
    #[prost(int64, tag = "1")]
    pub id: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseRevokeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseCheckpoint {
    /// ID is the lease ID to checkpoint.
    #[prost(int64, tag = "1")]
    pub id: i64,
    /// Remaining_TTL is the remaining time until expiry of the lease.
    #[prost(int64, tag = "2")]
    pub remaining_ttl: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseCheckpointRequest {
    #[prost(message, repeated, tag = "1")]
    pub checkpoints: ::prost::alloc::vec::Vec<LeaseCheckpoint>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseCheckpointResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseKeepAliveRequest {
    /// ID is the lease ID for the lease to keep alive.
    #[prost(int64, tag = "1")]
    pub id: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseKeepAliveResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// ID is the lease ID from the keep alive request.
    #[prost(int64, tag = "2")]
    pub id: i64,
    /// TTL is the new time-to-live for the lease.
    #[prost(int64, tag = "3")]
    pub ttl: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseTimeToLiveRequest {
    /// ID is the lease ID for the lease.
    #[prost(int64, tag = "1")]
    pub id: i64,
    /// keys is true to query all the keys attached to this lease.
    #[prost(bool, tag = "2")]
    pub keys: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseTimeToLiveResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// ID is the lease ID from the keep alive request.
    #[prost(int64, tag = "2")]
    pub id: i64,
    /// TTL is the remaining TTL in seconds for the lease; the lease will expire in under TTL+1 seconds.
    #[prost(int64, tag = "3")]
    pub ttl: i64,
    /// GrantedTTL is the initial granted time in seconds upon lease creation/renewal.
    #[prost(int64, tag = "4")]
    pub granted_ttl: i64,
    /// Keys is the list of keys attached to this lease.
    #[prost(bytes = "vec", repeated, tag = "5")]
    pub keys: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseLeasesRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseStatus {
    /// TODO: int64 TTL = 2;
    #[prost(int64, tag = "1")]
    pub id: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaseLeasesResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub leases: ::prost::alloc::vec::Vec<LeaseStatus>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Member {
    /// ID is the member ID for this member.
    #[prost(uint64, tag = "1")]
    pub id: u64,
    /// name is the human-readable name of the member. If the member is not started, the name will be an empty string.
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    /// peerURLs is the list of URLs the member exposes to the cluster for communication.
    #[prost(string, repeated, tag = "3")]
    pub peer_ur_ls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// clientURLs is the list of URLs the member exposes to clients for communication. If the member is not started, clientURLs will be empty.
    #[prost(string, repeated, tag = "4")]
    pub client_ur_ls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// isLearner indicates if the member is raft learner.
    #[prost(bool, tag = "5")]
    pub is_learner: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberAddRequest {
    /// peerURLs is the list of URLs the added member will use to communicate with the cluster.
    #[prost(string, repeated, tag = "1")]
    pub peer_ur_ls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// isLearner indicates if the added member is raft learner.
    #[prost(bool, tag = "2")]
    pub is_learner: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberAddResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// member is the member information for the added member.
    #[prost(message, optional, tag = "2")]
    pub member: ::core::option::Option<Member>,
    /// members is a list of all members after adding the new member.
    #[prost(message, repeated, tag = "3")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberRemoveRequest {
    /// ID is the member ID of the member to remove.
    #[prost(uint64, tag = "1")]
    pub id: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberRemoveResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// members is a list of all members after removing the member.
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberUpdateRequest {
    /// ID is the member ID of the member to update.
    #[prost(uint64, tag = "1")]
    pub id: u64,
    /// peerURLs is the new list of URLs the member will use to communicate with the cluster.
    #[prost(string, repeated, tag = "2")]
    pub peer_ur_ls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberUpdateResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// members is a list of all members after updating the member.
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberListRequest {
    #[prost(bool, tag = "1")]
    pub linearizable: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberListResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// members is a list of all members associated with the cluster.
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberPromoteRequest {
    /// ID is the member ID of the member to promote.
    #[prost(uint64, tag = "1")]
    pub id: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MemberPromoteResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// members is a list of all members after promoting the member.
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DefragmentRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DefragmentResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MoveLeaderRequest {
    /// targetID is the node ID for the new leader.
    #[prost(uint64, tag = "1")]
    pub target_id: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MoveLeaderResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AlarmRequest {
    /// action is the kind of alarm request to issue. The action
    /// may GET alarm statuses, ACTIVATE an alarm, or DEACTIVATE a
    /// raised alarm.
    #[prost(enumeration = "alarm_request::AlarmAction", tag = "1")]
    pub action: i32,
    /// memberID is the ID of the member associated with the alarm. If memberID is 0, the
    /// alarm request covers all members.
    #[prost(uint64, tag = "2")]
    pub member_id: u64,
    /// alarm is the type of alarm to consider for this request.
    #[prost(enumeration = "AlarmType", tag = "3")]
    pub alarm: i32,
}
/// Nested message and enum types in `AlarmRequest`.
pub mod alarm_request {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum AlarmAction {
        Get = 0,
        Activate = 1,
        Deactivate = 2,
    }
    impl AlarmAction {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                AlarmAction::Get => "GET",
                AlarmAction::Activate => "ACTIVATE",
                AlarmAction::Deactivate => "DEACTIVATE",
            }
        }
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AlarmMember {
    /// memberID is the ID of the member associated with the raised alarm.
    #[prost(uint64, tag = "1")]
    pub member_id: u64,
    /// alarm is the type of alarm which has been raised.
    #[prost(enumeration = "AlarmType", tag = "2")]
    pub alarm: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AlarmResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// alarms is a list of alarms associated with the alarm request.
    #[prost(message, repeated, tag = "2")]
    pub alarms: ::prost::alloc::vec::Vec<AlarmMember>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DowngradeRequest {
    /// action is the kind of downgrade request to issue. The action may
    /// VALIDATE the target version, DOWNGRADE the cluster version,
    /// or CANCEL the current downgrading job.
    #[prost(enumeration = "downgrade_request::DowngradeAction", tag = "1")]
    pub action: i32,
    /// version is the target version to downgrade.
    #[prost(string, tag = "2")]
    pub version: ::prost::alloc::string::String,
}
/// Nested message and enum types in `DowngradeRequest`.
pub mod downgrade_request {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum DowngradeAction {
        Validate = 0,
        Enable = 1,
        Cancel = 2,
    }
    impl DowngradeAction {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                DowngradeAction::Validate => "VALIDATE",
                DowngradeAction::Enable => "ENABLE",
                DowngradeAction::Cancel => "CANCEL",
            }
        }
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DowngradeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// version is the current cluster version.
    #[prost(string, tag = "2")]
    pub version: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StatusRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StatusResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// version is the cluster protocol version used by the responding member.
    #[prost(string, tag = "2")]
    pub version: ::prost::alloc::string::String,
    /// dbSize is the size of the backend database physically allocated, in bytes, of the responding member.
    #[prost(int64, tag = "3")]
    pub db_size: i64,
    /// leader is the member ID which the responding member believes is the current leader.
    #[prost(uint64, tag = "4")]
    pub leader: u64,
    /// raftIndex is the current raft committed index of the responding member.
    #[prost(uint64, tag = "5")]
    pub raft_index: u64,
    /// raftTerm is the current raft term of the responding member.
    #[prost(uint64, tag = "6")]
    pub raft_term: u64,
    /// raftAppliedIndex is the current raft applied index of the responding member.
    #[prost(uint64, tag = "7")]
    pub raft_applied_index: u64,
    /// errors contains alarm/health information and status.
    #[prost(string, repeated, tag = "8")]
    pub errors: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// dbSizeInUse is the size of the backend database logically in use, in bytes, of the responding member.
    #[prost(int64, tag = "9")]
    pub db_size_in_use: i64,
    /// isLearner indicates if the member is raft learner.
    #[prost(bool, tag = "10")]
    pub is_learner: bool,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthEnableRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthDisableRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthStatusRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthenticateRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub password: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserAddRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub password: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub options: ::core::option::Option<authpb::UserAddOptions>,
    #[prost(string, tag = "4")]
    pub hashed_password: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserGetRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserDeleteRequest {
    /// name is the name of the user to delete.
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserChangePasswordRequest {
    /// name is the name of the user whose password is being changed.
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    /// password is the new password for the user. Note that this field will be removed in the API layer.
    #[prost(string, tag = "2")]
    pub password: ::prost::alloc::string::String,
    /// hashedPassword is the new password for the user. Note that this field will be initialized in the API layer.
    #[prost(string, tag = "3")]
    pub hashed_password: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserGrantRoleRequest {
    /// user is the name of the user which should be granted a given role.
    #[prost(string, tag = "1")]
    pub user: ::prost::alloc::string::String,
    /// role is the name of the role to grant to the user.
    #[prost(string, tag = "2")]
    pub role: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserRevokeRoleRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub role: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleAddRequest {
    /// name is the name of the role to add to the authentication system.
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleGetRequest {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserListRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleListRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleDeleteRequest {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleGrantPermissionRequest {
    /// name is the name of the role which will be granted the permission.
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    /// perm is the permission to grant to the role.
    #[prost(message, optional, tag = "2")]
    pub perm: ::core::option::Option<authpb::Permission>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleRevokePermissionRequest {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "2")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthEnableResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthDisableResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthStatusResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(bool, tag = "2")]
    pub enabled: bool,
    /// authRevision is the current revision of auth store
    #[prost(uint64, tag = "3")]
    pub auth_revision: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthenticateResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    /// token is an authorized token that can be used in succeeding RPCs
    #[prost(string, tag = "2")]
    pub token: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserAddResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserGetResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, repeated, tag = "2")]
    pub roles: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserDeleteResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserChangePasswordResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserGrantRoleResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserRevokeRoleResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleAddResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleGetResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub perm: ::prost::alloc::vec::Vec<authpb::Permission>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleListResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, repeated, tag = "2")]
    pub roles: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthUserListResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, repeated, tag = "2")]
    pub users: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleDeleteResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleGrantPermissionResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthRoleRevokePermissionResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum AlarmType {
    /// default, used to query if any alarm is active
    None = 0,
    /// space quota is exhausted
    Nospace = 1,
    /// kv store corruption detected
    Corrupt = 2,
}
impl AlarmType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            AlarmType::None => "NONE",
            AlarmType::Nospace => "NOSPACE",
            AlarmType::Corrupt => "CORRUPT",
        }
    }
}

use futures::StreamExt;
use liegrpc::{
    client::GrpcClient,
    grpc::{Request, Response, Streaming},
};

#[tokio::main]
async fn main() {
    let uri = "http://127.0.0.1:2379";
    let mut client = liegrpc::client::Client::new([uri].iter()).unwrap();

    let mut req = RangeRequest::default();

    req.key = "/hello".as_bytes().to_vec();

    let req = Request::new(req);

    let rsp: Response<RangeResponse> = client
        .unary_unary("/etcdserverpb.KV/Range", req)
        .await
        .unwrap();

    let msg = rsp.get_ref();

    println!("=> {:?}", msg);

    let request = WatchCreateRequest {
        key: "/hello".as_bytes().to_vec(),
        ..Default::default()
    };

    let create_watch = watch_request::RequestUnion::CreateRequest(request);
    let create_req = WatchRequest {
        request_union: Some(create_watch),
    };

    let (req_tx, req_rx) = tokio::sync::mpsc::channel::<WatchRequest>(1);
    req_tx.send(create_req).await.unwrap();

    let rx = tokio_stream::wrappers::ReceiverStream::new(req_rx);

    let req = Request::new(rx);

    let mut rsp: Response<Streaming<WatchResponse>> = client
        .streaming_streaming("/etcdserverpb.Watch/Watch", req)
        .await
        .unwrap();

    let mut message_stream = rsp.message_stream();

    while let Some(m) = message_stream.next().await {
        println!("watch => {:?}", m);
    }


}
