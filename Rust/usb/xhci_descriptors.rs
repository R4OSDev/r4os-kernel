//! Exact bounded descriptor selection and catalogue projection. No allocation,
//! callback, wait or controller state outside the supplied owner is involved.
use super::xhci_state::Context;
use super::{
    types::Interface,
    xhci_types::{DescriptorInterface, Status},
};
use crate::sched::sync_types::Text;
fn le16(_ctx: Context, bytes: &[u8], offset: usize) -> u16 {
    u16::from(bytes[offset]) | (u16::from(bytes[offset + 1]) << 8)
}
fn bump(_ctx: Context, value: &mut u8) {
    *value = value.saturating_add(1);
}
pub(super) fn parse_device(ctx: Context, current: &mut Status, d: &[u8]) {
    current.descriptor_len = d[0];
    current.descriptor_type = d[1];
    current.usb_version_bcd = le16(ctx, d, 2);
    current.device_class = d[4];
    current.device_subclass = d[5];
    current.device_protocol = d[6];
    current.device_max_packet0 = d[7];
    current.device_vendor_id = le16(ctx, d, 8);
    current.device_product_id = le16(ctx, d, 10);
    current.device_version_bcd = le16(ctx, d, 12);
    current.manufacturer_index = d[14];
    current.product_index = d[15];
    current.serial_index = d[16];
}
pub(super) fn mass_storage(_ctx: Context, c: &DescriptorInterface) -> bool {
    c.valid
        && c.class_code == 8
        && c.subclass == 6
        && c.protocol == 0x50
        && c.bulk_in_endpoint_address != 0
        && c.bulk_out_endpoint_address != 0
}
pub(super) fn boot_hid(_ctx: Context, c: &DescriptorInterface) -> bool {
    c.valid
        && c.class_code == 3
        && c.subclass == 1
        && (c.protocol == 1 || c.protocol == 2)
        && c.interrupt_in_endpoint_address != 0
}
fn rank(ctx: Context, c: &DescriptorInterface) -> u8 {
    if mass_storage(ctx, c) {
        4
    } else if boot_hid(ctx, c) {
        3
    } else if c.valid && c.first_endpoint_address != 0 {
        2
    } else {
        u8::from(c.valid)
    }
}
fn reason(ctx: Context, c: &DescriptorInterface) -> Text {
    Text::literal(if mass_storage(ctx, c) {
        b"mass-storage-bulk"
    } else if boot_hid(ctx, c) {
        if c.protocol == 1 {
            b"hid-boot-keyboard"
        } else if c.protocol == 2 {
            b"hid-boot-mouse"
        } else {
            b"hid-boot"
        }
    } else if c.valid && c.first_endpoint_address != 0 {
        b"first-endpoint-interface"
    } else if c.valid {
        b"first-interface"
    } else {
        b"none"
    })
}
fn select(ctx: Context, c: &DescriptorInterface, selected: &mut DescriptorInterface) {
    if c.valid && rank(ctx, c) > rank(ctx, selected) {
        *selected = *c;
    }
}
fn endpoint(
    ctx: Context,
    current: &mut Status,
    c: &mut DescriptorInterface,
    address: u8,
    attributes: u8,
    packet: u16,
    interval: u8,
) {
    if !c.valid {
        bump(ctx, &mut current.descriptor_malformed);
        return;
    }
    bump(ctx, &mut c.endpoint_count);
    c.last_endpoint_address = address;
    if c.first_endpoint_address == 0 {
        c.first_endpoint_address = address;
        c.first_endpoint_attributes = attributes;
        c.first_endpoint_max_packet = packet;
        c.first_endpoint_interval = interval;
    }
    let transfer = attributes & 3;
    if transfer == 3 && address & 0x80 != 0 && c.interrupt_in_endpoint_address == 0 {
        c.interrupt_in_endpoint_address = address;
        c.interrupt_in_endpoint_max_packet = packet;
        c.interrupt_in_endpoint_interval = interval;
    } else if transfer == 2 {
        if address & 0x80 != 0 && c.bulk_in_endpoint_address == 0 {
            c.bulk_in_endpoint_address = address;
            c.bulk_in_endpoint_max_packet = packet;
        } else if address & 0x80 == 0 && c.bulk_out_endpoint_address == 0 {
            c.bulk_out_endpoint_address = address;
            c.bulk_out_endpoint_max_packet = packet;
        }
    }
}
fn companion(ctx: Context, current: &mut Status, c: &mut DescriptorInterface, d: &[u8], len: u8) {
    if !c.valid || len < 6 || c.last_endpoint_address == 0 {
        bump(ctx, &mut current.descriptor_malformed);
        c.last_endpoint_address = 0;
        return;
    }
    let burst = d[2];
    if burst > 15 {
        bump(ctx, &mut current.descriptor_malformed);
        c.last_endpoint_address = 0;
        return;
    }
    let address = c.last_endpoint_address;
    if address == c.bulk_in_endpoint_address {
        c.bulk_in_endpoint_max_burst = burst;
    } else if address == c.bulk_out_endpoint_address {
        c.bulk_out_endpoint_max_burst = burst;
    }
    c.last_endpoint_address = 0;
}
fn hid(ctx: Context, current: &mut Status, c: &mut DescriptorInterface, d: &[u8], len: u8) {
    if !c.valid {
        bump(ctx, &mut current.descriptor_malformed);
        return;
    }
    c.hid_descriptor_len = len;
    if len < 9 {
        bump(ctx, &mut current.descriptor_malformed);
        return;
    }
    let count = d[5];
    let (mut offset, mut seen) = (6usize, 0u8);
    while seen < count && offset + 3 <= usize::from(len) {
        let kind = d[offset];
        let length = le16(ctx, d, offset + 1);
        if kind == 0x22 && c.hid_report_descriptor_len == 0 {
            c.hid_report_descriptor_len = length;
        }
        offset += 3;
        seen += 1;
    }
}
fn publish_selected(ctx: Context, current: &mut Status, c: &DescriptorInterface) {
    current.selected_interface_reason = reason(ctx, c);
    current.first_interface_number = c.number;
    current.first_interface_class = c.class_code;
    current.first_interface_subclass = c.subclass;
    current.first_interface_protocol = c.protocol;
    current.first_endpoint_address = c.first_endpoint_address;
    current.first_endpoint_attributes = c.first_endpoint_attributes;
    current.first_endpoint_max_packet = c.first_endpoint_max_packet;
    current.first_endpoint_interval = c.first_endpoint_interval;
    current.first_hid_descriptor_len = c.hid_descriptor_len;
    current.first_hid_report_descriptor_len = c.hid_report_descriptor_len;
    if boot_hid(ctx, c) {
        current.first_endpoint_address = c.interrupt_in_endpoint_address;
        current.first_endpoint_attributes = 3;
        current.first_endpoint_max_packet = c.interrupt_in_endpoint_max_packet;
        current.first_endpoint_interval = c.interrupt_in_endpoint_interval;
    }
    if mass_storage(ctx, c) {
        current.bulk_in_endpoint_address = c.bulk_in_endpoint_address;
        current.bulk_in_endpoint_max_packet = c.bulk_in_endpoint_max_packet;
        current.bulk_in_endpoint_max_burst = c.bulk_in_endpoint_max_burst;
        current.bulk_out_endpoint_address = c.bulk_out_endpoint_address;
        current.bulk_out_endpoint_max_packet = c.bulk_out_endpoint_max_packet;
        current.bulk_out_endpoint_max_burst = c.bulk_out_endpoint_max_burst;
    }
}
fn record_interface(ctx: Context, current: &mut Status, c: &DescriptorInterface) {
    if !c.valid {
        return;
    }
    if usize::from(current.interface_record_count) >= 8 {
        bump(ctx, &mut current.descriptor_malformed);
        return;
    }
    let index = usize::from(current.interface_record_count);
    current.interface_records[index] = Interface {
        active: true,
        number: c.number,
        class_code: c.class_code,
        subclass: c.subclass,
        protocol: c.protocol,
        endpoint_count: c.endpoint_count,
        first_endpoint_address: c.first_endpoint_address,
        first_endpoint_attributes: c.first_endpoint_attributes,
        first_endpoint_max_packet: c.first_endpoint_max_packet,
        first_endpoint_interval: c.first_endpoint_interval,
        hid_descriptor_len: c.hid_descriptor_len,
        hid_report_descriptor_len: c.hid_report_descriptor_len,
        interrupt_in_endpoint_address: c.interrupt_in_endpoint_address,
        interrupt_in_endpoint_max_packet: c.interrupt_in_endpoint_max_packet,
        interrupt_in_endpoint_interval: c.interrupt_in_endpoint_interval,
        bulk_in_endpoint_address: c.bulk_in_endpoint_address,
        bulk_in_endpoint_max_burst: c.bulk_in_endpoint_max_burst,
        bulk_out_endpoint_address: c.bulk_out_endpoint_address,
        bulk_out_endpoint_max_burst: c.bulk_out_endpoint_max_burst,
    };
    current.interface_record_count += 1;
}
pub(super) fn parse_config(ctx: Context, current: &mut Status, d: &[u8], total: u16) {
    current.config_total_length = le16(ctx, d, 2);
    current.interface_count = d[4];
    current.interface_record_count = 0;
    current.interface_records = [Interface::EMPTY; 8];
    current.config_value = d[5];
    current.config_attributes = d[7];
    current.config_max_power_ma = u16::from(d[8]) * 2;
    current.endpoint_count = 0;
    current.descriptor_records = 0;
    current.descriptor_unknown = 0;
    current.descriptor_malformed = 0;
    current.hid_descriptor_count = 0;
    current.ss_endpoint_companion_count = 0;
    current.selected_interface_reason = Text::literal(b"none");
    current.first_interface_number = 0;
    current.first_interface_class = 0;
    current.first_interface_subclass = 0;
    current.first_interface_protocol = 0;
    current.first_endpoint_address = 0;
    current.first_endpoint_attributes = 0;
    current.first_endpoint_max_packet = 0;
    current.first_endpoint_interval = 0;
    current.first_hid_descriptor_len = 0;
    current.first_hid_report_descriptor_len = 0;
    current.bulk_in_endpoint_address = 0;
    current.bulk_in_endpoint_max_packet = 0;
    current.bulk_in_endpoint_max_burst = 0;
    current.bulk_out_endpoint_address = 0;
    current.bulk_out_endpoint_max_packet = 0;
    current.bulk_out_endpoint_max_burst = 0;
    let (mut offset, limit) = (9usize, usize::from(total));
    let mut active = DescriptorInterface::EMPTY;
    let mut selected = DescriptorInterface::EMPTY;
    while offset + 2 <= limit {
        let len = d[offset];
        let kind = d[offset + 1];
        if len < 2 || offset + usize::from(len) > limit {
            bump(ctx, &mut current.descriptor_malformed);
            break;
        }
        bump(ctx, &mut current.descriptor_records);
        if kind != 5 && kind != 0x30 {
            active.last_endpoint_address = 0;
        }
        if kind == 4 {
            if len >= 9 {
                select(ctx, &active, &mut selected);
                record_interface(ctx, current, &active);
                active = DescriptorInterface {
                    valid: true,
                    number: d[offset + 2],
                    class_code: d[offset + 5],
                    subclass: d[offset + 6],
                    protocol: d[offset + 7],
                    ..DescriptorInterface::EMPTY
                };
            } else {
                bump(ctx, &mut current.descriptor_malformed);
            }
        } else if kind == 5 {
            if len < 7 {
                bump(ctx, &mut current.descriptor_malformed);
                offset += usize::from(len);
                continue;
            }
            bump(ctx, &mut current.endpoint_count);
            endpoint(
                ctx,
                current,
                &mut active,
                d[offset + 2],
                d[offset + 3],
                le16(ctx, d, offset + 4),
                d[offset + 6],
            );
        } else if kind == 0x21 {
            bump(ctx, &mut current.hid_descriptor_count);
            hid(ctx, current, &mut active, &d[offset..], len);
        } else if kind == 0x30 {
            bump(ctx, &mut current.ss_endpoint_companion_count);
            companion(ctx, current, &mut active, &d[offset..], len);
        } else if kind != 2 && kind != 3 && kind != 1 {
            bump(ctx, &mut current.descriptor_unknown);
        }
        offset += usize::from(len);
    }
    select(ctx, &active, &mut selected);
    record_interface(ctx, current, &active);
    publish_selected(ctx, current, &selected);
}
