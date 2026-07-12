package alpha

pub error Same
error Hidden

pub fn fail() !void {
    return error.Same
}

pub fn hidden() !void {
    return error.Hidden
}
