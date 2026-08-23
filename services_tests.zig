const services = @import("kernel/services.zig");

test "service module reset tests are reachable" {
    _ = services.API_VERSION;
}
