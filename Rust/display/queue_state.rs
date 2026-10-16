//! Original bounded graphics timeline/physical-ownership model. The common BO
//! owner serializes metadata. Completion publication, callbacks and release run
//! outside this model; logical cancellation never invents physical quiescence.
use super::error::Error;
use crate::gfx::types::Owner;
use core::{
    mem::{MaybeUninit, size_of},
    slice,
};
pub const MAX_DEPENDENCIES: usize = 8;
pub const PENDING: u32 = 0;
pub const COMPLETE: u32 = 1;
pub const CANCELLED: u32 = 2;
pub const DROPPED: u32 = 3;
pub const DEVICE_LOST: u32 = 4;
pub const TIMEOUT: u32 = 5;
pub const FAILED: u32 = 6;
pub const DEPENDENCY_FAILED: u32 = 7;
pub const QUEUED: u32 = 0;
pub const RUNNING: u32 = 1;
pub const TERMINAL: u32 = 2;
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub adapter: u32,
    pub device_generation: u64,
    pub reset_generation: u64,
}
impl Default for Binding {
    fn default() -> Self {
        Self {
            adapter: 0,
            device_generation: 1,
            reset_generation: 1,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Fence {
    pub slot: u32,
    pub timeline: u64,
    pub point: u64,
    pub binding: Binding,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Config {
    pub binding: Binding,
    pub policy: u32,
    pub capacity: u32,
    pub milestone: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            binding: Binding::default(),
            policy: 0,
            capacity: 2,
            milestone: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Status {
    pub fence: Fence,
    pub phase: u32,
    pub result: u32,
    pub milestone: u32,
    pub deadline_ns: u64,
    pub completed_ns: u64,
    pub device_active: bool,
    pub resources_held: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Release {
    pub fence: Fence,
    pub nonce: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Queue {
    pub timeline: u64,
    pub owner: Owner,
    pub config: Config,
    pub next_point: u64,
    pub closing: bool,
    pub inflight: u32,
    pub jobs: u32,
    pub has_last_owner: bool,
    pub last_owner_data: Owner,
    pub next_owner_queue: usize,
}
impl Default for Queue {
    fn default() -> Self {
        Self {
            timeline: 0,
            owner: Owner::EMPTY,
            config: Config::default(),
            next_point: 1,
            closing: false,
            inflight: 0,
            jobs: 0,
            has_last_owner: false,
            last_owner_data: Owner::EMPTY,
            next_owner_queue: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Job {
    pub fence: Fence,
    pub queue: usize,
    pub phase: u32,
    pub result: u32,
    pub deadline_ns: u64,
    pub completed_ns: u64,
    pub frame_key: u64,
    pub client_reference: bool,
    pub active: bool,
    pub scanout: bool,
    pub retire_requested: bool,
    pub resources_held: bool,
    pub release_nonce: u64,
    pub notification_pending: bool,
    pub notification_publishing: bool,
    pub waiters: u32,
    pub dependents: u32,
    pub dependencies: [u16; MAX_DEPENDENCIES],
    pub dependency_count: usize,
}
impl Default for Job {
    fn default() -> Self {
        Self {
            fence: Fence::default(),
            queue: 0,
            phase: QUEUED,
            result: PENDING,
            deadline_ns: 0,
            completed_ns: 0,
            frame_key: 0,
            client_reference: true,
            active: false,
            scanout: false,
            retire_requested: false,
            resources_held: true,
            release_nonce: 0,
            notification_pending: false,
            notification_publishing: false,
            waiters: 0,
            dependents: 0,
            dependencies: [0; MAX_DEPENDENCIES],
            dependency_count: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Scalars {
    pub timeline_serial: u64,
    pub release_serial: u64,
    pub next_queue: usize,
    pub has_last_owner: bool,
    pub last_owner_data: Owner,
    pub terminal_publications: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawView {
    pub queues: *mut Queue,
    pub queues_len: usize,
    pub jobs: *mut Job,
    pub jobs_len: usize,
    pub scalars: *mut Scalars,
}
pub struct Table<'a> {
    pub queues: &'a mut [Queue],
    pub jobs: &'a mut [Job],
    pub scalars: &'a mut Scalars,
}
impl RawView {
    /// # Safety
    /// Disjoint initialized resident metadata is exclusively admitted by the
    /// common BO owner for this nonblocking view; no replacement/callback/wait.
    pub(crate) unsafe fn borrow<'a>(self) -> Table<'a> {
        assert!(self.queues_len != 0 && self.jobs_len != 0 && self.jobs_len <= 65535);
        unsafe {
            Table {
                queues: slice::from_raw_parts_mut(self.queues, self.queues_len),
                jobs: slice::from_raw_parts_mut(self.jobs, self.jobs_len),
                scalars: &mut *self.scalars,
            }
        }
    }
}
fn owner_before(left: Owner, right: Owner) -> bool {
    (left.kind, left.id, left.generation) < (right.kind, right.id, right.generation)
}
impl Table<'_> {
    pub fn open(&mut self, owner: Owner, config: Config) -> Result<u64, Error> {
        assert!(
            config.policy <= 1 && config.milestone <= 2,
            "graphics queue config enum"
        );
        if owner.id == 0
            || owner.generation == 0
            || config.capacity == 0
            || config.capacity as usize > self.jobs.len()
            || config.binding.device_generation == 0
            || config.binding.reset_generation == 0
        {
            return Err(Error::Invalid);
        }
        if self.scalars.timeline_serial == u64::MAX {
            return Err(Error::Exhausted);
        }
        let mut cursor = None;
        let mut owner_cursor = 0;
        for queue in self.queues.iter() {
            if queue.timeline != 0 && queue.config.binding == config.binding {
                cursor = if queue.has_last_owner {
                    Some(queue.last_owner_data)
                } else {
                    None
                };
                break;
            }
        }
        for queue in self.queues.iter() {
            if queue.timeline != 0 && queue.owner == owner && queue.config.binding == config.binding
            {
                owner_cursor = queue.next_owner_queue;
                break;
            }
        }
        for queue in self.queues.iter_mut() {
            if queue.timeline == 0 {
                self.scalars.timeline_serial += 1;
                *queue = Queue {
                    timeline: self.scalars.timeline_serial,
                    owner,
                    config,
                    has_last_owner: cursor.is_some(),
                    last_owner_data: cursor.unwrap_or(Owner::EMPTY),
                    next_owner_queue: owner_cursor,
                    ..Queue::default()
                };
                return Ok(queue.timeline);
            }
        }
        Err(Error::Capacity)
    }
    pub fn submit(
        &mut self,
        timeline: u64,
        owner: Owner,
        deadline: u64,
        frame_key: u64,
        dependencies: &[Fence],
        now: u64,
    ) -> Result<Fence, Error> {
        let qi = self.queue_for(timeline, owner)?;
        let queue = self.queues[qi];
        if queue.closing {
            return Err(Error::Closed);
        }
        if deadline <= now
            || deadline == u64::MAX
            || dependencies.len() > MAX_DEPENDENCIES
            || (queue.config.policy == 1 && frame_key == 0)
        {
            return Err(Error::Invalid);
        }
        if queue.next_point == u64::MAX {
            return Err(Error::Exhausted);
        }
        let mut indices = [0u16; MAX_DEPENDENCIES];
        for (index, &dependency) in dependencies.iter().enumerate() {
            let slot = self.fence_index(dependency)?;
            if indices[..index].contains(&(slot as u16)) {
                return Err(Error::Invalid);
            }
            if self.jobs[slot].dependents == u32::MAX {
                return Err(Error::Exhausted);
            }
            indices[index] = u16::try_from(slot).expect("graphics dependency slot");
        }
        let mut replacement = None;
        if queue.config.policy == 1 {
            for (index, job) in self.jobs.iter().enumerate() {
                if job.fence.slot != 0
                    && job.queue == qi
                    && job.phase == QUEUED
                    && job.frame_key == frame_key
                {
                    replacement = Some(index)
                }
            }
        }
        if replacement.is_none() && queue.inflight >= queue.config.capacity {
            return Err(Error::Busy);
        }
        let slot = self
            .jobs
            .iter()
            .position(|job| job.fence.slot == 0)
            .ok_or(Error::Capacity)?;
        if let Some(old) = replacement {
            self.make_terminal(old, DROPPED, now)
        }
        let fence = Fence {
            slot: (slot + 1) as u32,
            timeline,
            point: queue.next_point,
            binding: queue.config.binding,
        };
        self.queues[qi].next_point += 1;
        self.queues[qi].inflight += 1;
        self.queues[qi].jobs += 1;
        self.jobs[slot] = Job {
            fence,
            queue: qi,
            deadline_ns: deadline,
            frame_key,
            dependencies: indices,
            dependency_count: dependencies.len(),
            ..Job::default()
        };
        for &dependency in &indices[..dependencies.len()] {
            self.jobs[dependency as usize].dependents += 1
        }
        Ok(fence)
    }
    pub fn query(&self, fence: Fence) -> Result<Status, Error> {
        let job = self.jobs[self.fence_index(fence)?];
        Ok(Status {
            fence: job.fence,
            phase: job.phase,
            result: job.result,
            milestone: self.queues[job.queue].config.milestone,
            deadline_ns: job.deadline_ns,
            completed_ns: job.completed_ns,
            device_active: job.active || job.scanout,
            resources_held: job.resources_held,
        })
    }
    pub fn configuration(&self, timeline: u64, owner: Owner) -> Result<Config, Error> {
        let queue = self.queues[self.queue_for(timeline, owner)?];
        if queue.closing {
            Err(Error::Closed)
        } else {
            Ok(queue.config)
        }
    }
    pub fn orders(
        &self,
        timeline: u64,
        dependencies: &[Fence],
        before: Fence,
        seen: &mut [bool],
        pending: &mut [MaybeUninit<u16>],
    ) -> Result<bool, Error> {
        let _ = self.fence_index(before)?;
        if timeline == before.timeline {
            return Ok(true);
        }
        assert!(seen.len() >= self.jobs.len() && pending.len() >= self.jobs.len());
        seen[..self.jobs.len()].fill(false);
        let mut count = 0;
        for &fence in dependencies {
            let slot = self.fence_index(fence)?;
            if !seen[slot] {
                seen[slot] = true;
                pending[count].write(slot as u16);
                count += 1
            }
        }
        while count != 0 {
            count -= 1;
            let job = self.jobs[unsafe { pending[count].assume_init() } as usize];
            if job.fence.timeline == before.timeline && job.fence.point >= before.point {
                return Ok(true);
            }
            for &slot in &job.dependencies[..job.dependency_count] {
                let slot = slot as usize;
                if !seen[slot] {
                    seen[slot] = true;
                    pending[count].write(slot as u16);
                    count += 1
                }
            }
        }
        Ok(false)
    }
    pub fn producer(&self, fence: Fence) -> Result<Owner, Error> {
        Ok(self.queues[self.jobs[self.fence_index(fence)?].queue].owner)
    }
    // Original receipt/validation branch scopes retained during the port.
    #[allow(clippy::collapsible_if)]
    pub fn take_ready_for(&mut self, binding: Option<Binding>, now: u64) -> Option<Fence> {
        self.expire(now);
        let mut selected: Option<usize> = None;
        let mut selected_after = false;
        for visited in 0..self.queues.len() {
            let qi = (self.scalars.next_queue + visited) % self.queues.len();
            let queue = self.queues[qi];
            if queue.timeline == 0 || queue.closing {
                continue;
            }
            if let Some(wanted) = binding {
                if wanted != queue.config.binding {
                    continue;
                }
            }
            let mut first: Option<usize> = None;
            let mut active = false;
            for (index, job) in self.jobs.iter().enumerate() {
                if job.fence.slot == 0 || job.queue != qi {
                    continue;
                }
                if job.active {
                    active = true
                }
                if job.phase != QUEUED {
                    continue;
                }
                if first.is_none() || job.fence.point < self.jobs[first.unwrap()].fence.point {
                    first = Some(index)
                }
            }
            if active {
                continue;
            }
            let Some(slot) = first else { continue };
            let job = self.jobs[slot];
            let mut blocked = false;
            let mut failed = false;
            for &dependency in &job.dependencies[..job.dependency_count] {
                let parent = self.jobs[dependency as usize];
                if parent.phase != TERMINAL {
                    blocked = true
                } else if parent.result != COMPLETE {
                    failed = true
                } else if parent.active || parent.scanout {
                    blocked = true
                }
            }
            if failed {
                self.make_terminal(slot, DEPENDENCY_FAILED, now);
                continue;
            }
            if blocked {
                continue;
            }
            let cursor = if binding.is_some() {
                if queue.has_last_owner {
                    Some(queue.last_owner_data)
                } else {
                    None
                }
            } else if self.scalars.has_last_owner {
                Some(self.scalars.last_owner_data)
            } else {
                None
            };
            let after = cursor.is_none_or(|owner| owner_before(owner, queue.owner));
            if let Some(previous) = selected {
                let previous_owner = self.queues[self.jobs[previous].queue].owner;
                if queue.owner == previous_owner {
                    let start = if binding.is_some() {
                        queue.next_owner_queue
                    } else {
                        self.scalars.next_queue
                    };
                    let distance = (qi + self.queues.len() - start) % self.queues.len();
                    let prior =
                        (self.jobs[previous].queue + self.queues.len() - start) % self.queues.len();
                    if distance >= prior {
                        continue;
                    }
                } else if (!after && selected_after)
                    || (after == selected_after && !owner_before(queue.owner, previous_owner))
                {
                    continue;
                }
            }
            selected = Some(slot);
            selected_after = after;
        }
        let slot = selected?;
        let qi = self.jobs[slot].queue;
        let queue = self.queues[qi];
        self.jobs[slot].phase = RUNNING;
        self.jobs[slot].active = true;
        self.scalars.next_queue = (qi + 1) % self.queues.len();
        self.scalars.has_last_owner = true;
        self.scalars.last_owner_data = queue.owner;
        for peer in self.queues.iter_mut() {
            if peer.timeline != 0 && peer.config.binding == queue.config.binding {
                peer.has_last_owner = true;
                peer.last_owner_data = queue.owner;
                if peer.owner == queue.owner {
                    peer.next_owner_queue = self.scalars.next_queue
                }
            }
        }
        Some(self.jobs[slot].fence)
    }
    pub fn complete(
        &mut self,
        fence: Fence,
        result: u32,
        quiesced: bool,
        now: u64,
    ) -> Result<(), Error> {
        if !matches!(result, COMPLETE | FAILED | CANCELLED) {
            return Err(Error::Invalid);
        }
        let slot = self.fence_index(fence)?;
        let job = self.jobs[slot];
        if job.phase == QUEUED {
            return Err(Error::Invalid);
        }
        if !job.active && !job.scanout {
            return Err(Error::AlreadyCompleted);
        }
        if !quiesced {
            return Err(Error::Busy);
        }
        self.jobs[slot].active = false;
        self.jobs[slot].scanout = false;
        self.queues[job.queue].inflight -= 1;
        if job.phase != TERMINAL {
            self.make_terminal(slot, result, now)
        }
        Ok(())
    }
    pub fn begin_scanout(&mut self, fence: Fence, now: u64) -> Result<(), Error> {
        let slot = self.fence_index(fence)?;
        let job = self.jobs[slot];
        if job.scanout {
            return Ok(());
        }
        if !job.active || job.phase == QUEUED {
            return Err(Error::Invalid);
        }
        self.jobs[slot].active = false;
        self.jobs[slot].scanout = true;
        if job.phase != TERMINAL {
            self.make_terminal(slot, COMPLETE, now)
        } else {
            self.jobs[slot].retire_requested = true
        }
        Ok(())
    }
    pub fn scanout_retire_requested(&self, fence: Fence) -> Result<bool, Error> {
        let job = self.jobs[self.fence_index(fence)?];
        if !job.active && !job.scanout {
            return Err(Error::AlreadyCompleted);
        }
        Ok(job.retire_requested
            || !job.client_reference
            || self.queues[job.queue].closing
            || (job.phase == TERMINAL && job.result != COMPLETE))
    }
    pub fn cancel(&mut self, fence: Fence, owner: Owner, now: u64) -> Result<(), Error> {
        let slot = self.fence_index(fence)?;
        if self.queues[self.jobs[slot].queue].owner != owner {
            return Err(Error::WrongOwner);
        }
        if self.jobs[slot].scanout {
            self.jobs[slot].retire_requested = true;
            return Ok(());
        }
        if self.jobs[slot].phase == TERMINAL {
            return Err(Error::AlreadyCompleted);
        }
        self.make_terminal(slot, CANCELLED, now);
        Ok(())
    }
    pub fn expire(&mut self, now: u64) {
        for slot in 0..self.jobs.len() {
            let job = self.jobs[slot];
            if job.fence.slot != 0 && job.phase != TERMINAL && job.deadline_ns <= now {
                self.make_terminal(slot, TIMEOUT, now)
            }
        }
    }
    pub fn close(&mut self, timeline: u64, owner: Owner, now: u64) -> Result<(), Error> {
        let qi = self.queue_for(timeline, owner)?;
        self.queues[qi].closing = true;
        for slot in 0..self.jobs.len() {
            let job = self.jobs[slot];
            if job.fence.slot == 0 || job.queue != qi {
                continue;
            }
            self.jobs[slot].client_reference = false;
            if job.scanout {
                self.jobs[slot].retire_requested = true
            }
            if job.phase != TERMINAL {
                self.make_terminal(slot, CANCELLED, now)
            }
        }
        if self.queues[qi].jobs == 0 {
            self.queues[qi] = Queue::default()
        }
        Ok(())
    }
    pub fn stopped(&mut self, owner: Owner, now: u64) {
        for index in 0..self.queues.len() {
            let queue = self.queues[index];
            if queue.timeline != 0 && queue.owner == owner {
                self.close(queue.timeline, owner, now)
                    .expect("graphics owner queue")
            }
        }
    }
    pub fn device_lost(&mut self, binding: Binding, now: u64) {
        for qi in 0..self.queues.len() {
            if self.queues[qi].timeline == 0 || self.queues[qi].config.binding != binding {
                continue;
            }
            self.queues[qi].closing = true;
            for slot in 0..self.jobs.len() {
                let job = self.jobs[slot];
                if job.fence.slot != 0 && job.queue == qi {
                    if job.scanout {
                        self.jobs[slot].retire_requested = true
                    }
                    if job.phase != TERMINAL {
                        self.make_terminal(slot, DEVICE_LOST, now)
                    }
                }
            }
            if self.queues[qi].jobs == 0 {
                self.queues[qi] = Queue::default()
            }
        }
    }
    pub fn drop_reference(&mut self, fence: Fence, owner: Owner) -> Result<(), Error> {
        let slot = self.fence_index(fence)?;
        let job = self.jobs[slot];
        if self.queues[job.queue].owner != owner {
            return Err(Error::WrongOwner);
        }
        if !job.client_reference {
            return Err(Error::Stale);
        }
        self.jobs[slot].client_reference = false;
        Ok(())
    }
    pub fn retain_waiter(&mut self, fence: Fence) -> Result<(), Error> {
        let slot = self.fence_index(fence)?;
        self.jobs[slot].waiters = self.jobs[slot]
            .waiters
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        Ok(())
    }
    pub fn release_waiter(&mut self, fence: Fence) -> Result<(), Error> {
        let slot = self.fence_index(fence)?;
        if self.jobs[slot].waiters == 0 {
            return Err(Error::Invalid);
        }
        self.jobs[slot].waiters -= 1;
        Ok(())
    }
    pub fn take_notification(&mut self) -> Option<Fence> {
        for job in self.jobs.iter_mut() {
            if job.fence.slot != 0 && job.notification_pending && !job.notification_publishing {
                job.notification_publishing = true;
                return Some(job.fence);
            }
        }
        None
    }
    pub fn published(&mut self, fence: Fence) -> Result<(), Error> {
        let slot = self.fence_index(fence)?;
        if !self.jobs[slot].notification_publishing {
            return Err(Error::Invalid);
        }
        self.jobs[slot].notification_publishing = false;
        self.jobs[slot].notification_pending = false;
        Ok(())
    }
    pub fn take_release(&mut self) -> Option<Release> {
        if self.scalars.release_serial == u64::MAX {
            return None;
        }
        for job in self.jobs.iter_mut() {
            if job.fence.slot == 0
                || job.phase != TERMINAL
                || job.active
                || job.scanout
                || !job.resources_held
                || job.release_nonce != 0
            {
                continue;
            }
            self.scalars.release_serial += 1;
            job.release_nonce = self.scalars.release_serial;
            return Some(Release {
                fence: job.fence,
                nonce: job.release_nonce,
            });
        }
        None
    }
    pub fn released(&mut self, ticket: Release, success: bool) -> Result<(), Error> {
        let slot = self.fence_index(ticket.fence)?;
        let job = &mut self.jobs[slot];
        if ticket.nonce == 0 || job.release_nonce != ticket.nonce {
            return Err(Error::Stale);
        }
        job.release_nonce = 0;
        if success {
            job.resources_held = false
        }
        Ok(())
    }
    pub fn reap_one(&mut self) -> Option<Fence> {
        for slot in 0..self.jobs.len() {
            let job = self.jobs[slot];
            if job.fence.slot == 0
                || job.phase != TERMINAL
                || job.active
                || job.client_reference
                || job.resources_held
                || job.notification_pending
                || job.notification_publishing
                || job.waiters != 0
                || job.dependents != 0
            {
                continue;
            }
            for &dependency in &job.dependencies[..job.dependency_count] {
                self.jobs[dependency as usize].dependents -= 1
            }
            self.jobs[slot] = Job::default();
            self.queues[job.queue].jobs -= 1;
            if self.queues[job.queue].closing && self.queues[job.queue].jobs == 0 {
                self.queues[job.queue] = Queue::default()
            }
            return Some(job.fence);
        }
        None
    }
    fn make_terminal(&mut self, slot: usize, result: u32, now: u64) {
        let job = &mut self.jobs[slot];
        assert!(job.phase != TERMINAL && result != PENDING);
        if job.phase == QUEUED {
            self.queues[job.queue].inflight -= 1
        }
        job.phase = TERMINAL;
        job.result = result;
        job.completed_ns = now;
        job.notification_pending = true;
        self.scalars.terminal_publications = self.scalars.terminal_publications.saturating_add(1);
    }
    fn queue_for(&self, timeline: u64, owner: Owner) -> Result<usize, Error> {
        if timeline == 0 {
            return Err(Error::Invalid);
        }
        for (index, queue) in self.queues.iter().enumerate() {
            if queue.timeline == timeline {
                if queue.owner != owner {
                    return Err(Error::WrongOwner);
                }
                return Ok(index);
            }
        }
        Err(Error::Stale)
    }
    fn fence_index(&self, fence: Fence) -> Result<usize, Error> {
        if fence.slot == 0
            || fence.slot as usize > self.jobs.len()
            || fence.timeline == 0
            || fence.point == 0
        {
            return Err(Error::Invalid);
        }
        let slot = fence.slot as usize - 1;
        if self.jobs[slot].fence != fence {
            return Err(Error::Stale);
        }
        Ok(slot)
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<Binding>() == 24
            && size_of::<Fence>() == 48
            && size_of::<Config>() == 40
            && size_of::<Status>() == 88
            && size_of::<Release>() == 56
    );
    assert!(
        size_of::<Queue>() == 128
            && align_of::<Queue>() == 8
            && offset_of!(Queue, last_owner_data) == 96
            && offset_of!(Queue, next_owner_queue) == 120
    );
    assert!(
        size_of::<Job>() == 144
            && offset_of!(Job, release_nonce) == 96
            && offset_of!(Job, dependencies) == 116
            && offset_of!(Job, dependency_count) == 136
    );
    assert!(
        size_of::<Scalars>() == 64
            && offset_of!(Scalars, last_owner_data) == 32
            && offset_of!(Scalars, terminal_publications) == 56
            && size_of::<RawView>() == 40
    );
};

#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub config: Config,
    pub fence: Fence,
    pub binding: Binding,
    pub timeline: u64,
    pub now: u64,
    pub frame_key: u64,
    pub deadline: u64,
    pub nonce: u64,
    pub result: u32,
    pub flag: bool,
    pub has_binding: bool,
    pub dependencies: *const Fence,
    pub dependencies_len: usize,
    pub seen: *mut bool,
    pub pending: *mut MaybeUninit<u16>,
}
#[repr(C)]
#[derive(Default)]
pub struct OptionalFence {
    pub has: bool,
    pub value: Fence,
}
impl From<Option<Fence>> for OptionalFence {
    fn from(value: Option<Fence>) -> Self {
        Self {
            has: value.is_some(),
            value: value.unwrap_or_default(),
        }
    }
}
#[repr(C)]
#[derive(Default)]
pub struct OptionalRelease {
    pub has: bool,
    pub value: Release,
}
impl From<Option<Release>> for OptionalRelease {
    fn from(value: Option<Release>) -> Self {
        Self {
            has: value.is_some(),
            value: value.unwrap_or_default(),
        }
    }
}
enum Output {
    None,
    Number(u64),
    Fence(Fence),
    Status(Status),
    Config(Config),
    Boolean(bool),
    OptionalFence(OptionalFence),
    Owner(Owner),
    OptionalRelease(OptionalRelease),
}
impl Args {
    unsafe fn dependencies(&self) -> &[Fence] {
        if self.dependencies_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.dependencies, self.dependencies_len) }
        }
    }
}
/// # Safety
/// Exact disjoint initialized C metadata under the BO owner; arguments retain
/// their spans and orders scratch through this call. Output is a disjoint exact
/// result record. No callback, allocation, wait or replacement occurs here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_queue_state(
    view: *const RawView,
    op: u32,
    args: *const Args,
    out: *mut core::ffi::c_void,
) -> i32 {
    let args = unsafe { &*args };
    let mut table = unsafe { view.read().borrow() };
    let result = match op {
        0 => table.open(args.owner, args.config).map(Output::Number),
        1 => table
            .submit(
                args.timeline,
                args.owner,
                args.deadline,
                args.frame_key,
                unsafe { args.dependencies() },
                args.now,
            )
            .map(Output::Fence),
        2 => table.query(args.fence).map(Output::Status),
        3 => table
            .configuration(args.timeline, args.owner)
            .map(Output::Config),
        4 => {
            let len = table.jobs.len();
            table
                .orders(
                    args.timeline,
                    unsafe { args.dependencies() },
                    args.fence,
                    unsafe { slice::from_raw_parts_mut(args.seen, len) },
                    unsafe { slice::from_raw_parts_mut(args.pending, len) },
                )
                .map(Output::Boolean)
        }
        5 => Ok(Output::OptionalFence(
            table
                .take_ready_for(
                    if args.has_binding {
                        Some(args.binding)
                    } else {
                        None
                    },
                    args.now,
                )
                .into(),
        )),
        6 => table.producer(args.fence).map(Output::Owner),
        7 => table
            .complete(args.fence, args.result, args.flag, args.now)
            .map(|()| Output::None),
        8 => table
            .begin_scanout(args.fence, args.now)
            .map(|()| Output::None),
        9 => table
            .scanout_retire_requested(args.fence)
            .map(Output::Boolean),
        10 => table
            .cancel(args.fence, args.owner, args.now)
            .map(|()| Output::None),
        11 => {
            table.expire(args.now);
            Ok(Output::None)
        }
        12 => table
            .close(args.timeline, args.owner, args.now)
            .map(|()| Output::None),
        13 => {
            table.stopped(args.owner, args.now);
            Ok(Output::None)
        }
        14 => {
            table.device_lost(args.binding, args.now);
            Ok(Output::None)
        }
        15 => table
            .drop_reference(args.fence, args.owner)
            .map(|()| Output::None),
        16 => table.retain_waiter(args.fence).map(|()| Output::None),
        17 => table.release_waiter(args.fence).map(|()| Output::None),
        18 => Ok(Output::OptionalFence(table.take_notification().into())),
        19 => table.published(args.fence).map(|()| Output::None),
        20 => Ok(Output::OptionalRelease(table.take_release().into())),
        21 => table
            .released(
                Release {
                    fence: args.fence,
                    nonce: args.nonce,
                },
                args.flag,
            )
            .map(|()| Output::None),
        22 => Ok(Output::OptionalFence(table.reap_one().into())),
        _ => panic!("graphics queue state operation"),
    };
    match result {
        Err(error) => error as i32,
        Ok(value) => {
            unsafe {
                match value {
                    Output::None => {}
                    Output::Number(v) => out.cast::<u64>().write(v),
                    Output::Fence(v) => out.cast::<Fence>().write(v),
                    Output::Status(v) => out.cast::<Status>().write(v),
                    Output::Config(v) => out.cast::<Config>().write(v),
                    Output::Boolean(v) => out.cast::<bool>().write(v),
                    Output::OptionalFence(v) => out.cast::<OptionalFence>().write(v),
                    Output::Owner(v) => out.cast::<Owner>().write(v),
                    Output::OptionalRelease(v) => out.cast::<OptionalRelease>().write(v),
                }
            };
            0
        }
    }
}
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Args>() == 216
            && offset_of!(Args, dependencies) == 184
            && offset_of!(Args, seen) == 200
            && offset_of!(Args, pending) == 208
    );
    assert!(size_of::<OptionalFence>() == 56 && size_of::<OptionalRelease>() == 64);
};
