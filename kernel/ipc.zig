// 0.56.32: Kompat-Shim - der Kanal-IPC-Code ist in das Service-IPC
// ueberfuehrt (kernel/service_ipc.zig, Befund 8.4). Reiner Struktur-Umzug
// ohne Verhaltens-/Groessenaenderungen; bestehende Importer (network_boot,
// net/core, net/ipc_services, program/r4net) laufen ueber diesen Shim.
// Neue Nutzer sollen direkt service_ipc.zig importieren.
const service_ipc = @import("service_ipc.zig");

pub const MAX_CHANNELS = service_ipc.MAX_CHANNELS;
pub const QUEUE_DEPTH = service_ipc.QUEUE_DEPTH;
pub const MAX_MESSAGE_SIZE = service_ipc.MAX_MESSAGE_SIZE;
pub const CHANNEL_ECHO = service_ipc.CHANNEL_ECHO;
pub const CHANNEL_NET_DHCP = service_ipc.CHANNEL_NET_DHCP;
pub const CHANNEL_NET_DNS = service_ipc.CHANNEL_NET_DNS;
pub const CHANNEL_NET_TCP = service_ipc.CHANNEL_NET_TCP;
pub const CHANNEL_NET_UDP = service_ipc.CHANNEL_NET_UDP;
pub const ServiceHandler = service_ipc.ServiceHandler;
pub const Summary = service_ipc.Summary;
pub const ChannelInfo = service_ipc.ChannelInfo;
pub const init = service_ipc.init;
pub const registerService = service_ipc.registerService;
pub const open = service_ipc.open;
pub const close = service_ipc.close;
pub const poll = service_ipc.poll;
pub const send = service_ipc.send;
pub const recv = service_ipc.recv;
pub const echoSmoke = service_ipc.echoSmoke;
pub const summary = service_ipc.summary;
pub const channelInfo = service_ipc.channelInfo;
pub const dumpStatus = service_ipc.dumpStatus;
