package env

pub error IO
pub error InvalidArgument
pub error NotFound
pub error PermissionDenied

pub fn lookup(name str) !str {
    panic("env.lookup intrinsic")
}
