//! Bounded connector/receiver catalog and atomic output admission. Copied
//! metadata only: no device access, callback, allocation, wait or BO release.
use super::{
    audio_routes, brightness_state, equal::same, error::Error, output_target,
    output_validation as checks, power_state, refresh_state,
};
use crate::{
    contract as a,
    gfx::{layout, types::Owner},
};
use core::{mem::size_of, ptr};
pub const CAPACITY: usize = a::gfx_output_catalog_capacity as usize;
const MODES: usize = a::gfx_output_max_modes as usize;
const EDID: usize = a::gfx_output_max_edid_bytes as usize;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Source {
    pub owner: Owner,
    pub binding: a::GfxReceiverSource,
    pub sequence: u64,
}
impl Default for Source {
    fn default() -> Self {
        Self {
            owner: Owner {
                kind: 2,
                id: 0,
                generation: 0,
            },
            binding: a::GfxReceiverSource::default(),
            sequence: 0,
        }
    }
}
#[repr(C)]
pub struct Entry {
    pub owner: u32,
    pub receiver_source: u64,
    pub paused: bool,
    pub info: a::GfxOutputInfo,
    pub has_color: bool,
    pub color_data: a::GfxOutputColorState,
    pub refresh: refresh_state::State,
    pub power: power_state::State,
    pub brightness: brightness_state::State,
    pub retained_sleep: bool,
    pub modes: [a::GfxOutputMode; MODES],
    pub edid: [u8; EDID],
}
impl Entry {
    /// # Safety
    /// Exact resident unobserved entry storage, or its exclusive admitted owner.
    pub unsafe fn init_in_place(out: *mut Self) {
        unsafe {
            ptr::addr_of_mut!((*out).owner).write(0);
            ptr::addr_of_mut!((*out).receiver_source).write(0);
            ptr::addr_of_mut!((*out).paused).write(false);
            ptr::addr_of_mut!((*out).info).write(a::GfxOutputInfo::default());
            ptr::addr_of_mut!((*out).has_color).write(false);
            ptr::addr_of_mut!((*out).color_data).write(a::GfxOutputColorState::default());
            ptr::addr_of_mut!((*out).refresh).write(refresh_state::State::default());
            ptr::addr_of_mut!((*out).power).write(power_state::State::default());
            ptr::addr_of_mut!((*out).brightness).write(brightness_state::State::default());
            ptr::addr_of_mut!((*out).retained_sleep).write(false);
            let modes = ptr::addr_of_mut!((*out).modes).cast::<a::GfxOutputMode>();
            for i in 0..MODES {
                modes.add(i).write(a::GfxOutputMode::default())
            }
            ptr::addr_of_mut!((*out).edid)
                .cast::<u8>()
                .write_bytes(0, EDID);
        }
    }
    fn reset(&mut self) {
        unsafe { Self::init_in_place(self) }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Fact {
    pub has_buffer: bool,
    pub buffer: layout::Descriptor,
    pub dependency: u32,
}
impl Default for Fact {
    fn default() -> Self {
        Self {
            has_buffer: false,
            buffer: layout::Descriptor::EMPTY,
            dependency: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Ticket {
    pub id: u64,
}
#[repr(C)]
pub struct Store {
    pub entries: [Entry; CAPACITY],
    pub audio: audio_routes::Store,
    pub sources: [Source; 16],
    pub source_serial: u64,
    pub revision: u64,
    pub receiver_serial: u64,
    pub attempt: u64,
    pub commit_sequence: u64,
    pub pending: Ticket,
    pub retained: u32,
    pub reset_retired: Ticket,
}
impl Store {
    /// # Safety
    /// Exact resident unpublished storage. Never form a full Store reference or
    /// move this large catalog through a task stack before initialization ends.
    pub unsafe fn init_in_place(out: *mut Self) {
        unsafe {
            let entries = ptr::addr_of_mut!((*out).entries).cast::<Entry>();
            for i in 0..CAPACITY {
                Entry::init_in_place(entries.add(i))
            }
            let routes = ptr::addr_of_mut!((*out).audio.records).cast::<a::GfxAudioRoute>();
            for i in 0..CAPACITY {
                routes.add(i).write(a::GfxAudioRoute::default())
            }
            let sources = ptr::addr_of_mut!((*out).sources).cast::<Source>();
            for i in 0..16 {
                sources.add(i).write(Source::default())
            }
            ptr::addr_of_mut!((*out).source_serial).write(0);
            ptr::addr_of_mut!((*out).revision).write(0);
            ptr::addr_of_mut!((*out).receiver_serial).write(0);
            ptr::addr_of_mut!((*out).attempt).write(0);
            ptr::addr_of_mut!((*out).commit_sequence).write(0);
            ptr::addr_of_mut!((*out).pending).write(Ticket::default());
            ptr::addr_of_mut!((*out).retained).write(0);
            ptr::addr_of_mut!((*out).reset_retired).write(Ticket::default());
        }
    }
    fn can_change(&self) -> Result<(), Error> {
        if self.pending.id != 0 || self.retained != 0 {
            return Err(Error::Busy);
        }
        if self.revision == u64::MAX || self.receiver_serial == u64::MAX {
            return Err(Error::Exhausted);
        }
        Ok(())
    }
    // Original receipt/validation branch scopes retained during the port.
    #[allow(clippy::collapsible_if)]
    pub fn publish(
        &mut self,
        owner: u32,
        info: a::GfxOutputInfo,
        modes: &[a::GfxOutputMode],
        edid: &[u8],
        promote: bool,
    ) -> Result<a::GfxOutputId, Error> {
        self.can_change()?;
        for source in &self.sources {
            if source.binding.generation != 0
                && source.binding.adapter_id == info.identity.adapter_id
                && source.owner.id != u64::from(owner)
            {
                return Err(Error::Busy);
            }
        }
        if info.version != 1
            || (info.size as usize) < size_of::<a::GfxOutputInfo>()
            || info.limits.version != 1
            || (info.limits.size as usize) < size_of::<a::GfxDisplayLimits>()
            || info.reserved0 != 0
            || info.reserved1 != 0
            || info.limits.reserved0 != 0
            || info.identity.connector_id == 0
            || info.identity.device_generation == 0
            || info.identity.connection_generation != 0
            || info.mode_count as usize != modes.len()
            || info.edid_bytes as usize != edid.len()
            || modes.len() > MODES
            || edid.len() > EDID
            || !edid.len().is_multiple_of(128)
            || info.flags
                & !(31
                    | a::gfx_output_flag_edid_missing
                    | a::gfx_output_flag_edid_invalid
                    | a::gfx_output_flag_receiver_incomplete
                    | a::gfx_output_flag_query_failed)
                != 0
            || info.connector_kind > a::gfx_output_kind_dvi
        {
            return Err(Error::Invalid);
        }
        if (owner == 0) != (info.identity.adapter_id == 0) {
            return Err(Error::Invalid);
        }
        if info.flags & a::gfx_output_flag_connected == 0 {
            if !edid.is_empty() {
                return Err(Error::Invalid);
            }
            if !modes.is_empty() || info.preferred_mode_id != 0 {
                if info.flags & a::gfx_output_flag_connection_unknown == 0
                    || modes.len() != 1
                    || modes[0].flags & a::gfx_output_mode_geometry_only == 0
                {
                    return Err(Error::Invalid);
                }
            }
        }
        checks::valid_limits(info.limits)?;
        if info.possible_heads == 0
            || info.possible_planes == 0
            || info.possible_plls == 0
            || info.possible_heads & !info.limits.head_mask != 0
            || info.possible_planes & !info.limits.plane_mask != 0
            || info.possible_plls & !info.limits.pll_mask != 0
        {
            return Err(Error::Routing);
        }
        let mut preferred = info.preferred_mode_id == 0;
        for (i, &mode) in modes.iter().enumerate() {
            if !checks::valid_mode(mode) {
                return Err(Error::Invalid);
            }
            if modes[..i].iter().any(|prior| prior.mode_id == mode.mode_id) {
                return Err(Error::Invalid);
            }
            if info.preferred_mode_id == mode.mode_id {
                preferred = true
            }
        }
        if !preferred {
            return Err(Error::Invalid);
        }
        let mut selected = None;
        let mut vacant = None;
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.info.identity.connector_id == 0 {
                if vacant.is_none() {
                    vacant = Some(index)
                }
                continue;
            }
            if entry.info.identity.adapter_id != info.identity.adapter_id {
                continue;
            }
            if entry.receiver_source == 0
                && entry.info.identity.device_generation == info.identity.device_generation
                && !same(&entry.info.limits, &info.limits)
            {
                let withdrawn = entry.info.flags == 0
                    && entry.info.mode_count == 0
                    && entry.info.edid_bytes == 0;
                if entry.owner != owner
                    || entry.info.identity.connector_id != info.identity.connector_id
                    || (!withdrawn
                        && (!promote
                            || entry.info.limits.flags & a::gfx_output_limit_modeset != 0
                            || info.limits.flags & a::gfx_output_limit_modeset == 0))
                {
                    return Err(Error::Invalid);
                }
            }
            if entry.info.identity.connector_id != info.identity.connector_id {
                continue;
            }
            if entry.owner != owner && entry.owner != 0 {
                return Err(Error::Busy);
            }
            selected = Some(index);
        }
        let index = selected.or(vacant).ok_or(Error::Capacity)?;
        let mut identity = info.identity;
        self.receiver_serial += 1;
        self.revision += 1;
        identity.connection_generation = self.receiver_serial;
        let entry = &mut self.entries[index];
        let paused = entry.owner == owner && entry.receiver_source == 0 && entry.paused;
        let sleeping = paused
            && entry.info.identity.device_generation == info.identity.device_generation
            && (entry.power.sleeping() || entry.retained_sleep);
        entry.reset();
        entry.owner = owner;
        entry.info = info;
        entry.paused = paused;
        entry.retained_sleep = sleeping;
        if paused {
            entry.info.flags &= !a::gfx_output_flag_active
        }
        entry.info.identity = identity;
        entry.info.topology_revision = self.revision;
        entry.modes[..modes.len()].copy_from_slice(modes);
        entry.edid[..edid.len()].copy_from_slice(edid);
        Ok(identity)
    }
    pub fn find(&self, identity: a::GfxOutputId) -> Result<usize, Error> {
        self.entries
            .iter()
            .position(|entry| {
                entry.info.identity.connector_id != 0 && same(&entry.info.identity, &identity)
            })
            .ok_or(Error::Stale)
    }
    pub fn info_at(&self, index: u32) -> Option<a::GfxOutputInfo> {
        let mut ordinal = 0;
        for entry in &self.entries {
            if entry.info.identity.connector_id == 0 {
                continue;
            }
            if ordinal == index {
                let mut result = entry.info;
                if entry.power.sleeping() || entry.retained_sleep {
                    result.flags |= a::gfx_output_flag_sleeping
                }
                result.topology_revision = self.revision;
                return Some(result);
            }
            ordinal += 1;
        }
        None
    }
    pub fn publish_color(
        &mut self,
        owner: u32,
        value: a::GfxOutputColorState,
    ) -> Result<bool, Error> {
        if owner == 0 || !checks::valid_color(&value) || value.revision != 0 {
            return Err(Error::Invalid);
        }
        let index = self
            .entries
            .iter()
            .position(|entry| same(&entry.info.identity, &value.identity))
            .ok_or(Error::Stale)?;
        let entry = &self.entries[index];
        if entry.owner != owner || entry.receiver_source != 0 {
            return Err(Error::Stale);
        }
        if value.flags & a::gfx_output_color_active != 0
            && (entry.paused || entry.info.flags & a::gfx_output_flag_active == 0)
        {
            return Err(Error::Stale);
        }
        if entry.has_color && same(&entry.color_data, &value) {
            return Ok(false);
        }
        self.can_change()?;
        self.entries[index].has_color = true;
        self.entries[index].color_data = value;
        self.revision += 1;
        Ok(true)
    }
    pub fn color_at(&self, identity: a::GfxOutputId) -> Result<a::GfxOutputColorState, Error> {
        let entry = &self.entries[self.find(identity)?];
        if !entry.has_color {
            return Err(Error::Unsupported);
        }
        let mut value = entry.color_data;
        if !same(&value.identity, &identity) {
            return Err(Error::Unsupported);
        }
        value.revision = self.revision;
        if entry.paused || entry.info.flags & a::gfx_output_flag_active == 0 {
            value.flags &= !a::gfx_output_color_active;
            value.format = 0;
            value.bpc = 0;
            value.primaries = 0;
            value.transfer = 0;
            value.range = 0;
            value.reference_white = 0;
            value.peak = 0;
            value.black = 0;
            value.dp_payload_bits_per_second = 0;
            value.link_kind = 0;
            value.link_flags = 0;
            value.compressed_bpp_x16 = 0;
            value.link_lanes = 0;
            value.link_rate_mbps = 0;
            value.link_payload_bits_per_second = 0;
            value.h_active = 0;
            value.h_total = 0;
            value.v_active = 0;
            value.pixel_clock_numerator = 0;
            value.pixel_clock_denominator = 0;
        }
        Ok(value)
    }
    pub fn mode_at(
        &self,
        identity: a::GfxOutputId,
        index: u32,
    ) -> Result<Option<a::GfxOutputMode>, Error> {
        let entry = &self.entries[self.find(identity)?];
        Ok(if index < entry.info.mode_count {
            Some(entry.modes[index as usize])
        } else {
            None
        })
    }
    pub fn refresh_entry(&self, target: a::GfxOutputTarget) -> Result<usize, Error> {
        if !output_target::valid(&target) {
            return Err(Error::Invalid);
        }
        let index = self.find(a::GfxOutputId {
            adapter_id: target.adapter_id,
            connector_id: target.connector_id,
            device_generation: target.device_generation,
            connection_generation: target.connection_generation,
        })?;
        let entry = &self.entries[index];
        if entry.owner == 0 || entry.receiver_source != 0 {
            return Err(Error::Unsupported);
        }
        Ok(index)
    }
    pub fn publish_refresh(
        &mut self,
        owner: u32,
        value: a::GfxOutputRefresh,
    ) -> Result<bool, Error> {
        let index = self.refresh_entry(value.target)?;
        let entry = &mut self.entries[index];
        if entry.owner != owner || owner == 0 {
            return Err(Error::Stale);
        }
        if value.status.phase == a::gfx_refresh_phase_active
            && (entry.paused || entry.info.flags & a::gfx_output_flag_active == 0)
        {
            return Err(Error::Stale);
        }
        entry.refresh.publish(value)
    }
    pub fn refresh_at(&self, target: a::GfxOutputTarget) -> Result<a::GfxOutputRefresh, Error> {
        let entry = &self.entries[self.refresh_entry(target)?];
        if entry.paused || entry.info.flags & a::gfx_output_flag_active == 0 {
            return Err(Error::Stale);
        }
        entry.refresh.get(target)
    }
    pub fn request_refresh(
        &mut self,
        actor: Owner,
        input: a::GfxRefreshRequest,
        now: u64,
    ) -> Result<a::GfxRefreshRequest, Error> {
        let index = self.refresh_entry(input.target)?;
        let entry = &mut self.entries[index];
        if entry.paused || entry.info.flags & a::gfx_output_flag_active == 0 {
            return Err(Error::Stale);
        }
        entry.refresh.request(actor, input, now)
    }
    pub fn read_refresh(
        &mut self,
        owner: u32,
        target: a::GfxOutputTarget,
        now: u64,
    ) -> Result<a::GfxRefreshRequest, Error> {
        let index = self.refresh_entry(target)?;
        let entry = &mut self.entries[index];
        if entry.owner != owner || owner == 0 {
            return Err(Error::Stale);
        }
        if entry.paused || entry.info.flags & a::gfx_output_flag_active == 0 {
            entry.refresh.pause()
        }
        entry.refresh.read(target, now)
    }
    pub fn power_entry(&self, identity: a::GfxOutputId) -> Result<usize, Error> {
        let index = self.find(identity)?;
        let entry = &self.entries[index];
        if entry.owner == 0 || entry.receiver_source != 0 {
            return Err(Error::Unsupported);
        }
        Ok(index)
    }
    pub fn publish_power(&mut self, owner: u32, value: a::GfxOutputPower) -> Result<bool, Error> {
        let index = self.power_entry(value.identity)?;
        let entry = &mut self.entries[index];
        if owner == 0 || owner != entry.owner {
            return Err(Error::Stale);
        }
        if value.phase == a::gfx_power_phase_on
            && (entry.paused || entry.info.flags & a::gfx_output_flag_active == 0)
        {
            return Err(Error::Stale);
        }
        if value.phase == a::gfx_power_phase_off && !entry.paused {
            return Err(Error::Stale);
        }
        let topology = (entry.power.sleeping() || entry.retained_sleep)
            != power_state::sleeping_phase(value.phase);
        if topology && self.revision >= u64::MAX - u64::from(self.pending.id != 0) {
            return Err(Error::Exhausted);
        }
        let changed = entry.power.publish(value)?;
        entry.retained_sleep = false;
        if topology {
            self.revision += 1
        }
        Ok(changed)
    }
    pub fn power_at(&self, identity: a::GfxOutputId) -> Result<a::GfxOutputPower, Error> {
        self.entries[self.power_entry(identity)?]
            .power
            .get(identity)
    }
    pub fn request_power(
        &mut self,
        actor: Owner,
        input: a::GfxPowerRequest,
        now: u64,
    ) -> Result<a::GfxPowerRequest, Error> {
        let i = self.power_entry(input.identity)?;
        self.entries[i].power.request(actor, input, now)
    }
    pub fn read_power(
        &mut self,
        owner: u32,
        identity: a::GfxOutputId,
        now: u64,
    ) -> Result<a::GfxPowerRequest, Error> {
        let i = self.power_entry(identity)?;
        if owner == 0 || self.entries[i].owner != owner {
            return Err(Error::Stale);
        }
        self.entries[i].power.read(identity, now)
    }
    pub fn publish_brightness(
        &mut self,
        owner: u32,
        value: a::GfxOutputBrightness,
    ) -> Result<bool, Error> {
        let i = self.power_entry(value.identity)?;
        if owner == 0 || self.entries[i].owner != owner {
            return Err(Error::Stale);
        }
        self.entries[i].brightness.publish(value)
    }
    pub fn brightness_at(&self, identity: a::GfxOutputId) -> Result<a::GfxOutputBrightness, Error> {
        self.entries[self.power_entry(identity)?]
            .brightness
            .get(identity)
    }
    pub fn request_brightness(
        &mut self,
        actor: Owner,
        input: a::GfxBrightnessRequest,
    ) -> Result<a::GfxBrightnessRequest, Error> {
        let i = self.power_entry(input.identity)?;
        if self.entries[i].paused || self.entries[i].info.flags & a::gfx_output_flag_active == 0 {
            return Err(Error::Busy);
        }
        self.entries[i].brightness.request(actor, input)
    }
    pub fn read_brightness(
        &self,
        owner: u32,
        identity: a::GfxOutputId,
    ) -> Result<a::GfxBrightnessRequest, Error> {
        let i = self.power_entry(identity)?;
        if owner == 0 || self.entries[i].owner != owner {
            return Err(Error::Stale);
        }
        self.entries[i].brightness.read(identity)
    }
    pub fn withdraw(&mut self, owner: u32, identity: a::GfxOutputId) -> Result<(), Error> {
        self.can_change()?;
        let index = self.find(identity)?;
        if owner == 0
            || self.entries[index].owner != owner
            || self.entries[index].receiver_source != 0
        {
            return Err(Error::Stale);
        }
        self.disconnect(index);
        Ok(())
    }
    pub fn pause(
        &mut self,
        owner: u32,
        identity: a::GfxOutputId,
        paused: bool,
        primary: bool,
    ) -> Result<bool, Error> {
        let index = self.find(identity)?;
        let entry = &mut self.entries[index];
        if owner == 0 || entry.owner != owner || entry.receiver_source != 0 {
            return Err(Error::Stale);
        }
        if entry.paused == paused {
            return Ok(false);
        }
        let ceiling = u64::MAX - u64::from(self.pending.id != 0);
        if self.revision >= ceiling {
            return Err(Error::Exhausted);
        }
        entry.paused = paused;
        if paused {
            entry.refresh.pause();
            entry.info.flags &= !a::gfx_output_flag_active
        } else if primary {
            entry.info.flags |= a::gfx_output_flag_active
        }
        self.revision += 1;
        entry.info.topology_revision = self.revision;
        Ok(true)
    }
    fn disconnect(&mut self, index: usize) {
        self.receiver_serial += 1;
        self.revision += 1;
        let entry = &mut self.entries[index];
        entry.info.identity.connection_generation = self.receiver_serial;
        entry.info.topology_revision = self.revision;
        entry.info.flags = 0;
        entry.info.mode_count = 0;
        entry.info.preferred_mode_id = 0;
        entry.info.edid_bytes = 0;
        entry.refresh = refresh_state::State::default();
        entry.power = power_state::State::default();
        entry.brightness = brightness_state::State::default();
        entry.retained_sleep = false;
        entry.modes.fill(a::GfxOutputMode::default());
        entry.edid.fill(0);
    }
    pub fn stop(&mut self, owner: u32) -> Result<bool, Error> {
        if owner == 0 {
            return Err(Error::Invalid);
        }
        let mut receivers_changed = false;
        for i in 0..self.sources.len() {
            if self.sources[i].binding.generation != 0
                && self.sources[i].owner.id == u64::from(owner)
            {
                receivers_changed =
                    self.erase_receivers(self.sources[i].binding.generation) || receivers_changed;
                self.sources[i] = Source::default();
            }
        }
        let count = self
            .entries
            .iter()
            .filter(|entry| entry.owner == owner)
            .count() as u64;
        let ceiling = u64::MAX - u64::from(self.pending.id != 0);
        if self.receiver_serial > u64::MAX - count || self.revision > ceiling - count {
            return Err(Error::Exhausted);
        }
        for i in 0..self.entries.len() {
            if self.entries[i].owner == owner {
                self.disconnect(i);
                self.entries[i].owner = 0
            }
        }
        Ok(count != 0 || receivers_changed)
    }
}

impl Store {
    fn retired_port(entry: &Entry) -> bool {
        entry.owner == 0
            && entry.receiver_source == 0
            && entry.info.identity.adapter_id != 0
            && entry.info.flags == 0
            && entry.info.mode_count == 0
            && entry.info.edid_bytes == 0
    }
    pub fn register_source(
        &mut self,
        owner: Owner,
        adapter: u32,
    ) -> Result<a::GfxReceiverSource, Error> {
        if owner.kind != 2 || !owner.valid() || owner.id > u64::from(u32::MAX) || adapter == 0 {
            return Err(Error::Invalid);
        }
        if self.source_serial == u64::MAX || self.revision == u64::MAX {
            return Err(Error::Exhausted);
        }
        for entry in &self.entries {
            if entry.info.identity.connector_id != 0
                && entry.info.identity.adapter_id == adapter
                && u64::from(entry.owner) != owner.id
                && !Self::retired_port(entry)
            {
                return Err(Error::Busy);
            }
        }
        if self
            .sources
            .iter()
            .any(|source| source.binding.generation != 0 && source.binding.adapter_id == adapter)
        {
            return Err(Error::Busy);
        }
        let index = self
            .sources
            .iter()
            .position(|source| source.binding.generation == 0)
            .ok_or(Error::Capacity)?;
        let mut reclaimed = false;
        for entry in &mut self.entries {
            if entry.info.identity.adapter_id == adapter && Self::retired_port(entry) {
                entry.reset();
                reclaimed = true
            }
        }
        if reclaimed {
            self.receiver_change()
        }
        self.source_serial += 1;
        self.sources[index] = Source {
            owner,
            binding: a::GfxReceiverSource {
                adapter_id: adapter,
                generation: self.source_serial,
                ..a::GfxReceiverSource::default()
            },
            sequence: 0,
        };
        Ok(self.sources[index].binding)
    }
    fn receiver_source(&self, owner: Owner, binding: a::GfxReceiverSource) -> Result<usize, Error> {
        if binding.adapter_id == 0 || binding.generation == 0 || binding.reserved0 != 0 {
            return Err(Error::Invalid);
        }
        self.sources
            .iter()
            .position(|source| {
                source.binding.generation == binding.generation
                    && same(&source.binding, &binding)
                    && source.owner == owner
            })
            .ok_or(Error::Stale)
    }
    fn receiver_change(&mut self) {
        let ceiling = u64::MAX - u64::from(self.pending.id != 0);
        if self.revision < ceiling {
            self.revision += 1
        }
    }
    fn erase_receivers(&mut self, generation: u64) -> bool {
        let mut changed = self.audio.erase(generation);
        for entry in &mut self.entries {
            if entry.receiver_source == generation {
                entry.reset();
                changed = true
            }
        }
        if changed {
            self.receiver_change()
        }
        changed
    }
    pub fn close_source(
        &mut self,
        owner: Owner,
        binding: a::GfxReceiverSource,
    ) -> Result<bool, Error> {
        let index = self.receiver_source(owner, binding)?;
        let changed = self.erase_receivers(binding.generation);
        self.sources[index] = Source::default();
        Ok(changed)
    }
    pub fn replace_receivers(
        &mut self,
        owner: Owner,
        binding: a::GfxReceiverSource,
        sequence: u64,
        records: &[a::GfxReceiverInfo],
    ) -> Result<(), Error> {
        let index = self.receiver_source(owner, binding)?;
        if sequence == 0 || sequence <= self.sources[index].sequence {
            return Err(Error::Stale);
        }
        if records.len() > a::gfx_receiver_max_outputs as usize {
            return Err(Error::Invalid);
        }
        let ceiling = u64::MAX - u64::from(self.pending.id != 0);
        if self.revision >= ceiling || self.receiver_serial > u64::MAX - records.len() as u64 {
            return Err(Error::Exhausted);
        }
        let available = self
            .entries
            .iter()
            .filter(|entry| {
                entry.info.identity.connector_id == 0 || entry.receiver_source == binding.generation
            })
            .count();
        let mut required = 0;
        for (i, record) in records.iter().enumerate() {
            checks::valid_receiver(record)?;
            if records[..i]
                .iter()
                .any(|prior| prior.connector_id == record.connector_id)
            {
                return Err(Error::Invalid);
            }
            if !self.source_owns_port(owner, binding.adapter_id, record.connector_id) {
                required += 1
            }
        }
        if required > available {
            return Err(Error::Capacity);
        }
        for entry in &mut self.entries {
            if entry.receiver_source == binding.generation {
                entry.reset()
            }
        }
        self.revision += 1;
        self.sources[index].sequence = sequence;
        self.audio.invalidate(binding.generation, sequence);
        let mut slot = 0;
        for record in records {
            if self.source_owns_port(owner, binding.adapter_id, record.connector_id) {
                continue;
            }
            while self.entries[slot].info.identity.connector_id != 0 {
                slot += 1
            }
            self.receiver_serial += 1;
            let entry = &mut self.entries[slot];
            slot += 1;
            entry.reset();
            entry.owner = u32::try_from(owner.id).expect("receiver driver id");
            entry.receiver_source = binding.generation;
            entry.info = a::GfxOutputInfo {
                identity: a::GfxOutputId {
                    adapter_id: binding.adapter_id,
                    device_generation: binding.generation,
                    connector_id: record.connector_id,
                    connection_generation: self.receiver_serial,
                },
                topology_revision: self.revision,
                flags: record.flags | a::gfx_output_flag_receiver_only,
                connector_kind: record.connector_kind,
                mode_count: record.mode_count,
                preferred_mode_id: record.preferred_mode_id,
                edid_bytes: record.edid_bytes,
                limits: checks::unknown_limits(),
                ..a::GfxOutputInfo::default()
            };
            entry.modes[..record.mode_count as usize]
                .copy_from_slice(&record.modes[..record.mode_count as usize]);
            entry.edid[..record.edid_bytes as usize]
                .copy_from_slice(&record.edid[..record.edid_bytes as usize]);
        }
        Ok(())
    }
    fn source_owns_port(&self, owner: Owner, adapter: u32, connector: u32) -> bool {
        self.entries.iter().any(|entry| {
            entry.receiver_source == 0
                && u64::from(entry.owner) == owner.id
                && entry.info.identity.adapter_id == adapter
                && entry.info.identity.connector_id == connector
        })
    }
    pub fn publish_audio(&mut self, owner: Owner, value: &a::GfxAudioRoute) -> Result<(), Error> {
        let index = self.receiver_source(owner, value.source)?;
        if value.receiver_sequence != self.sources[index].sequence {
            return Err(Error::Stale);
        }
        self.audio.publish(value)
    }
    pub fn cursor(&self) -> a::GfxDisplayRevision {
        a::GfxDisplayRevision {
            revision: self.revision,
            present: self
                .entries
                .iter()
                .filter(|entry| entry.info.identity.connector_id != 0)
                .count() as u32,
            ..a::GfxDisplayRevision::default()
        }
    }
    // Retain the original checked arithmetic and overflow boundary.
    #[allow(clippy::manual_div_ceil)]
    pub fn validate(
        &self,
        state: &a::GfxAtomicState,
        facts: &[Fact],
    ) -> Result<a::GfxAtomicResult, Error> {
        if state.version != 1
            || (state.size as usize) < size_of::<a::GfxAtomicState>()
            || state.reserved0 != 0
            || state.count == 0
            || state.count > a::gfx_output_max_assignments
            || facts.len() != state.count as usize
        {
            return Err(Error::Invalid);
        }
        if state.topology_revision != self.revision {
            return Err(Error::Stale);
        }
        if self.pending.id != 0 || self.retained != 0 {
            return Err(Error::Busy);
        }
        if state.assignments[state.count as usize..]
            .iter()
            .any(|entry| !same(entry, &a::GfxScanoutState::default()))
        {
            return Err(Error::Invalid);
        }
        let mut heads = 0u32;
        let mut planes = 0u32;
        let mut plls = 0u32;
        let mut bandwidth = 0u128;
        let mut clocks = 0u128;
        let adapter = state.assignments[0].output;
        let mut limits = a::GfxDisplayLimits::default();
        for (i, (assignment, fact)) in state.assignments[..state.count as usize]
            .iter()
            .zip(facts.iter())
            .enumerate()
        {
            assert!(fact.dependency <= 3, "output dependency projection");
            if assignment.version != 1
                || (assignment.size as usize) < size_of::<a::GfxScanoutState>()
                || assignment.reserved0 != 0
                || assignment.buffer.reserved0 != 0
                || assignment.output.adapter_id != adapter.adapter_id
                || assignment.output.device_generation != adapter.device_generation
            {
                return Err(Error::Invalid);
            }
            let entry = &self.entries[self.find(assignment.output)?];
            if entry.info.flags & a::gfx_output_flag_connected == 0 {
                return Err(Error::Stale);
            }
            if entry.info.flags & a::gfx_output_flag_receiver_only != 0 {
                return Err(Error::Unsupported);
            }
            if i == 0 {
                limits = entry.info.limits
            } else if !same(&limits, &entry.info.limits) {
                return Err(Error::Invalid);
            }
            if state.assignments[..i]
                .iter()
                .any(|prior| prior.output.connector_id == assignment.output.connector_id)
            {
                return Err(Error::Routing);
            }
            let hb = checks::bit(assignment.head_id)?;
            let pb = checks::bit(assignment.plane_id)?;
            let cb = checks::bit(assignment.pll_id)?;
            if hb & entry.info.possible_heads == 0
                || pb & entry.info.possible_planes == 0
                || cb & entry.info.possible_plls == 0
                || heads & hb != 0
                || planes & pb != 0
                || plls & cb != 0
            {
                return Err(Error::Routing);
            }
            heads |= hb;
            planes |= pb;
            plls |= cb;
            let chosen = entry.modes[..entry.info.mode_count as usize]
                .iter()
                .find(|mode| mode.mode_id == assignment.mode_id)
                .copied()
                .ok_or(Error::Stale)?;
            if assignment.rotation > 3
                || assignment.color > 3
                || assignment.bits_per_color > 16
                || assignment.bits_per_color < 6
                || limits.rotations & checks::bit(assignment.rotation)? == 0
                || limits.colors & checks::bit(assignment.color)? == 0
                || limits.bpc_mask & checks::bit(assignment.bits_per_color)? == 0
            {
                return Err(Error::Unsupported);
            }
            if chosen.flags & a::gfx_output_mode_interlaced != 0
                && limits.flags & a::gfx_output_limit_interlace == 0
            {
                return Err(Error::Unsupported);
            }
            if chosen.flags & a::gfx_output_mode_420_only != 0 && assignment.color != 3 {
                return Err(Error::Unsupported);
            }
            if assignment.color == 3
                && chosen.flags & (a::gfx_output_mode_420_only | a::gfx_output_mode_420_allowed)
                    == 0
            {
                return Err(Error::Unsupported);
            }
            if !checks::fits(
                chosen.width,
                assignment.destination_x,
                assignment.destination_width,
            ) || !checks::fits(
                chosen.height,
                assignment.destination_y,
                assignment.destination_height,
            ) {
                return Err(Error::Invalid);
            }
            let sw = if assignment.rotation & 1 == 0 {
                assignment.source_width
            } else {
                assignment.source_height
            };
            let sh = if assignment.rotation & 1 == 0 {
                assignment.source_height
            } else {
                assignment.source_width
            };
            if (sw != assignment.destination_width || sh != assignment.destination_height)
                && limits.flags & a::gfx_output_limit_scaling == 0
            {
                return Err(Error::Unsupported);
            }
            if assignment.source_width > limits.max_width
                || assignment.source_height > limits.max_height
                || chosen.width > limits.max_width
                || chosen.height > limits.max_height
            {
                return Err(Error::Unsupported);
            }
            if fact.dependency == 2 {
                return Err(Error::Busy);
            }
            if fact.dependency == 3 {
                return Err(Error::Dependency);
            }
            if same(&assignment.ready_fence, &a::GfxFence::default()) != (fact.dependency == 0) {
                return Err(Error::Invalid);
            }
            let firmware = entry.info.flags
                & (a::gfx_output_flag_fixed_geometry | a::gfx_output_flag_firmware_snapshot)
                == (a::gfx_output_flag_fixed_geometry | a::gfx_output_flag_firmware_snapshot);
            if firmware {
                if state.count != 1
                    || !same(&assignment.buffer, &a::GfxBufferHandle::default())
                    || fact.has_buffer
                    || fact.dependency != 0
                    || assignment.rotation != 0
                    || assignment.color != 0
                    || assignment.bits_per_color != 8
                    || assignment.source_x != 0
                    || assignment.source_y != 0
                    || assignment.destination_x != 0
                    || assignment.destination_y != 0
                    || assignment.source_width != chosen.width
                    || assignment.source_height != chosen.height
                    || assignment.destination_width != chosen.width
                    || assignment.destination_height != chosen.height
                {
                    return Err(Error::Unsupported);
                }
                continue;
            }
            if limits.flags & a::gfx_output_limit_modeset == 0
                || chosen.flags & a::gfx_output_mode_geometry_only != 0
            {
                return Err(Error::Unsupported);
            }
            if !fact.has_buffer {
                return Err(Error::Invalid);
            }
            let descriptor = fact.buffer;
            if assignment.buffer.id == 0 || assignment.buffer.generation == 0 {
                return Err(Error::Invalid);
            }
            let _ = layout::validate(descriptor, false).map_err(|_| Error::Invalid)?;
            if !descriptor.binding.portable()
                && (descriptor.binding.adapter != adapter.adapter_id
                    || descriptor.binding.device_generation != adapter.device_generation)
            {
                return Err(Error::Stale);
            }
            if descriptor.usage & 32 == 0 || descriptor.modifier != 0 || limits.modifiers & 1 == 0 {
                return Err(Error::Unsupported);
            }
            let format = match descriptor.format {
                layout::XRGB8888 => 1,
                layout::ARGB8888 => 2,
                _ => return Err(Error::Unsupported),
            };
            if limits.formats & format == 0
                || descriptor.plane_count != 1
                || descriptor.planes[0].pitch % u64::from(limits.pitch_alignment) != 0
            {
                return Err(Error::Unsupported);
            }
            if !checks::fits(
                descriptor.width,
                assignment.source_x,
                assignment.source_width,
            ) || !checks::fits(
                descriptor.height,
                assignment.source_y,
                assignment.source_height,
            ) {
                return Err(Error::Invalid);
            }
            clocks += u128::from(chosen.pixel_clock_hz);
            bandwidth += (u128::from(descriptor.planes[0].pitch)
                * u128::from(descriptor.height)
                * u128::from(chosen.refresh_millihz)
                + 999)
                / 1000;
            if chosen.pixel_clock_hz > limits.max_pixel_clock_hz
                || clocks > u128::from(limits.total_pixel_clock_hz)
                || bandwidth > u128::from(limits.bandwidth_bytes_per_second)
            {
                return Err(Error::Bandwidth);
            }
        }
        Ok(a::GfxAtomicResult {
            topology_revision: self.revision,
            commit_sequence: self.commit_sequence,
            ..a::GfxAtomicResult::default()
        })
    }
    pub fn begin(&mut self, state: &a::GfxAtomicState, facts: &[Fact]) -> Result<Ticket, Error> {
        let _ = self.validate(state, facts)?;
        self.can_change()?;
        if self.attempt == u64::MAX || self.commit_sequence == u64::MAX {
            return Err(Error::Exhausted);
        }
        self.attempt += 1;
        self.pending = Ticket { id: self.attempt };
        Ok(self.pending)
    }
    pub fn finish(
        &mut self,
        ticket: Ticket,
        outcome: u32,
        quiesced: u32,
    ) -> Result<a::GfxAtomicResult, Error> {
        if ticket.id == 0 || ticket.id != self.pending.id {
            return Err(Error::Stale);
        }
        if quiesced & !3 != 0 {
            return Err(Error::Invalid);
        }
        let retain = match outcome {
            a::gfx_output_outcome_applied => {
                if quiesced & 1 == 0 {
                    return Err(Error::Busy);
                }
                2
            }
            a::gfx_output_outcome_old_preserved => {
                if quiesced & 2 == 0 {
                    return Err(Error::Busy);
                }
                1
            }
            a::gfx_output_outcome_lost => 3 & !quiesced,
            _ => return Err(Error::Invalid),
        };
        self.pending = Ticket::default();
        self.revision += 1;
        if outcome == a::gfx_output_outcome_applied {
            self.commit_sequence += 1
        }
        if outcome == a::gfx_output_outcome_lost {
            self.retained = retain
        }
        Ok(a::GfxAtomicResult {
            topology_revision: self.revision,
            commit_sequence: self.commit_sequence,
            outcome,
            retained: retain,
            ..a::GfxAtomicResult::default()
        })
    }
    pub fn retire_after_reset(&mut self, ticket: Ticket) -> Result<(), Error> {
        if ticket.id == 0
            || ticket.id != self.attempt
            || (self.pending.id != 0 && self.pending.id != ticket.id)
        {
            return Err(Error::Stale);
        }
        if self.reset_retired.id == ticket.id {
            return Ok(());
        }
        if self.revision == u64::MAX {
            return Err(Error::Exhausted);
        }
        self.pending = Ticket::default();
        self.retained = 0;
        self.reset_retired = ticket;
        self.revision += 1;
        Ok(())
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<Source>() == 48
            && size_of::<Fact>() == 144
            && offset_of!(Fact, buffer) == 8
            && offset_of!(Fact, dependency) == 136
            && size_of::<Ticket>() == 8
    );
    assert!(
        size_of::<Entry>() == 9360
            && align_of::<Entry>() == 8
            && offset_of!(Entry, info) == 24
            && offset_of!(Entry, color_data) == 200
            && offset_of!(Entry, refresh) == 392
            && offset_of!(Entry, power) == 800
            && offset_of!(Entry, brightness) == 1008
            && offset_of!(Entry, modes) == 1168
            && offset_of!(Entry, edid) == 5264
    );
    assert!(
        size_of::<Store>() == 611136
            && align_of::<Store>() == 8
            && offset_of!(Store, audio) == 599040
            && offset_of!(Store, sources) == 610304
            && offset_of!(Store, source_serial) == 611072
            && offset_of!(Store, reset_retired) == 611128
    );
};

#[repr(C)]
pub struct Args {
    pub owner: u32,
    pub actor: Owner,
    pub identity: a::GfxOutputId,
    pub target: a::GfxOutputTarget,
    pub binding: a::GfxReceiverSource,
    pub sequence: u64,
    pub index: u32,
    pub ticket: Ticket,
    pub outcome: u32,
    pub quiesced: u32,
    pub flag: bool,
    pub other_flag: bool,
    pub now: u64,
    pub input: *const core::ffi::c_void,
    pub modes: *const a::GfxOutputMode,
    pub modes_len: usize,
    pub edid: *const u8,
    pub edid_len: usize,
    pub facts: *const Fact,
    pub facts_len: usize,
    pub records: *const a::GfxReceiverInfo,
    pub records_len: usize,
}
#[repr(C)]
pub struct OptionalInfo {
    pub has: bool,
    pub value: a::GfxOutputInfo,
}
impl From<Option<a::GfxOutputInfo>> for OptionalInfo {
    fn from(value: Option<a::GfxOutputInfo>) -> Self {
        Self {
            has: value.is_some(),
            value: value.unwrap_or_default(),
        }
    }
}
#[repr(C)]
pub struct OptionalMode {
    pub has: bool,
    pub value: a::GfxOutputMode,
}
impl From<Option<a::GfxOutputMode>> for OptionalMode {
    fn from(value: Option<a::GfxOutputMode>) -> Self {
        Self {
            has: value.is_some(),
            value: value.unwrap_or_default(),
        }
    }
}
enum Output {
    None,
    Id(a::GfxOutputId),
    Index(usize),
    Info(OptionalInfo),
    Boolean(bool),
    Color(a::GfxOutputColorState),
    Mode(OptionalMode),
    Refresh(a::GfxOutputRefresh),
    RefreshRequest(a::GfxRefreshRequest),
    Power(a::GfxOutputPower),
    PowerRequest(a::GfxPowerRequest),
    Brightness(a::GfxOutputBrightness),
    BrightnessRequest(a::GfxBrightnessRequest),
    Source(a::GfxReceiverSource),
    Cursor(a::GfxDisplayRevision),
    Atomic(a::GfxAtomicResult),
    Ticket(Ticket),
}
unsafe fn span<'a, T>(pointer: *const T, len: usize) -> &'a [T] {
    if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(pointer, len) }
    }
}
/// # Safety
/// Exact initialized C catalog under its admitted display owner. Read operations
/// never form a mutable view. Inputs/spans are stable and disjoint from the
/// catalog and result; copied facts project optionals explicitly. No callbacks,
/// allocation, resource operations or waiting occur inside this metadata call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_output_state(
    store: *mut Store,
    op: u32,
    args: *const Args,
    out: *mut core::ffi::c_void,
) -> i32 {
    let args = unsafe { &*args };
    let result = match op {
        1 => unsafe { &*store }.find(args.identity).map(Output::Index),
        2 => Ok(Output::Info(unsafe { &*store }.info_at(args.index).into())),
        4 => unsafe { &*store }
            .color_at(args.identity)
            .map(Output::Color),
        5 => unsafe { &*store }
            .mode_at(args.identity, args.index)
            .map(|v| Output::Mode(v.into())),
        6 => unsafe { &*store }
            .refresh_entry(args.target)
            .map(Output::Index),
        8 => unsafe { &*store }
            .refresh_at(args.target)
            .map(Output::Refresh),
        11 => unsafe { &*store }
            .power_entry(args.identity)
            .map(Output::Index),
        13 => unsafe { &*store }
            .power_at(args.identity)
            .map(Output::Power),
        17 => unsafe { &*store }
            .brightness_at(args.identity)
            .map(Output::Brightness),
        19 => unsafe { &*store }
            .read_brightness(args.owner, args.identity)
            .map(Output::BrightnessRequest),
        27 => Ok(Output::Cursor(unsafe { &*store }.cursor())),
        28 => unsafe { &*store }
            .validate(
                unsafe { &*args.input.cast::<a::GfxAtomicState>() },
                unsafe { span(args.facts, args.facts_len) },
            )
            .map(Output::Atomic),
        _ => {
            let store = unsafe { &mut *store };
            match op {
                0 => store
                    .publish(
                        args.owner,
                        unsafe { args.input.cast::<a::GfxOutputInfo>().read() },
                        unsafe { span(args.modes, args.modes_len) },
                        unsafe { span(args.edid, args.edid_len) },
                        args.flag,
                    )
                    .map(Output::Id),
                3 => store
                    .publish_color(args.owner, unsafe {
                        args.input.cast::<a::GfxOutputColorState>().read()
                    })
                    .map(Output::Boolean),
                7 => store
                    .publish_refresh(args.owner, unsafe {
                        args.input.cast::<a::GfxOutputRefresh>().read()
                    })
                    .map(Output::Boolean),
                9 => store
                    .request_refresh(
                        args.actor,
                        unsafe { args.input.cast::<a::GfxRefreshRequest>().read() },
                        args.now,
                    )
                    .map(Output::RefreshRequest),
                10 => store
                    .read_refresh(args.owner, args.target, args.now)
                    .map(Output::RefreshRequest),
                12 => store
                    .publish_power(args.owner, unsafe {
                        args.input.cast::<a::GfxOutputPower>().read()
                    })
                    .map(Output::Boolean),
                14 => store
                    .request_power(
                        args.actor,
                        unsafe { args.input.cast::<a::GfxPowerRequest>().read() },
                        args.now,
                    )
                    .map(Output::PowerRequest),
                15 => store
                    .read_power(args.owner, args.identity, args.now)
                    .map(Output::PowerRequest),
                16 => store
                    .publish_brightness(args.owner, unsafe {
                        args.input.cast::<a::GfxOutputBrightness>().read()
                    })
                    .map(Output::Boolean),
                18 => store
                    .request_brightness(args.actor, unsafe {
                        args.input.cast::<a::GfxBrightnessRequest>().read()
                    })
                    .map(Output::BrightnessRequest),
                20 => store
                    .withdraw(args.owner, args.identity)
                    .map(|()| Output::None),
                21 => store
                    .pause(args.owner, args.identity, args.flag, args.other_flag)
                    .map(Output::Boolean),
                22 => store.stop(args.owner).map(Output::Boolean),
                23 => store
                    .register_source(args.actor, args.index)
                    .map(Output::Source),
                24 => store
                    .close_source(args.actor, args.binding)
                    .map(Output::Boolean),
                25 => store
                    .replace_receivers(args.actor, args.binding, args.sequence, unsafe {
                        span(args.records, args.records_len)
                    })
                    .map(|()| Output::None),
                26 => store
                    .publish_audio(args.actor, unsafe {
                        &*args.input.cast::<a::GfxAudioRoute>()
                    })
                    .map(|()| Output::None),
                29 => store
                    .begin(
                        unsafe { &*args.input.cast::<a::GfxAtomicState>() },
                        unsafe { span(args.facts, args.facts_len) },
                    )
                    .map(Output::Ticket),
                30 => store
                    .finish(args.ticket, args.outcome, args.quiesced)
                    .map(Output::Atomic),
                31 => store.retire_after_reset(args.ticket).map(|()| Output::None),
                _ => panic!("output catalog operation"),
            }
        }
    };
    match result {
        Err(error) => error as i32,
        Ok(value) => {
            unsafe {
                match value {
                    Output::None => {}
                    Output::Id(v) => out.cast::<a::GfxOutputId>().write(v),
                    Output::Index(v) => out.cast::<usize>().write(v),
                    Output::Info(v) => out.cast::<OptionalInfo>().write(v),
                    Output::Boolean(v) => out.cast::<bool>().write(v),
                    Output::Color(v) => out.cast::<a::GfxOutputColorState>().write(v),
                    Output::Mode(v) => out.cast::<OptionalMode>().write(v),
                    Output::Refresh(v) => out.cast::<a::GfxOutputRefresh>().write(v),
                    Output::RefreshRequest(v) => out.cast::<a::GfxRefreshRequest>().write(v),
                    Output::Power(v) => out.cast::<a::GfxOutputPower>().write(v),
                    Output::PowerRequest(v) => out.cast::<a::GfxPowerRequest>().write(v),
                    Output::Brightness(v) => out.cast::<a::GfxOutputBrightness>().write(v),
                    Output::BrightnessRequest(v) => out.cast::<a::GfxBrightnessRequest>().write(v),
                    Output::Source(v) => out.cast::<a::GfxReceiverSource>().write(v),
                    Output::Cursor(v) => out.cast::<a::GfxDisplayRevision>().write(v),
                    Output::Atomic(v) => out.cast::<a::GfxAtomicResult>().write(v),
                    Output::Ticket(v) => out.cast::<Ticket>().write(v),
                }
            };
            0
        }
    }
}
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Args>() == 240
            && offset_of!(Args, actor) == 8
            && offset_of!(Args, input) == 168
            && offset_of!(Args, records_len) == 232
            && size_of::<OptionalInfo>() == 176
            && size_of::<OptionalMode>() == 72
    );
};
