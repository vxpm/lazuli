use std::collections::VecDeque;

use gekko::Cycles;

use crate::system::System;

pub struct HandlerCtx {
    pub cycles_late: Cycles,
}

pub type BasicHandler = fn(&mut System);
pub type FullHandler = fn(&mut System, HandlerCtx);

#[derive(Clone, Copy)]
pub enum Handler {
    Basic(BasicHandler),
    Full(FullHandler),
}

impl PartialEq for Handler {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Basic(f), Self::Basic(g)) => std::ptr::fn_addr_eq(*f, *g),
            (Self::Full(f), Self::Full(g)) => std::ptr::fn_addr_eq(*f, *g),
            _ => false,
        }
    }
}

impl Eq for Handler {}

impl Handler {
    #[inline(always)]
    pub fn call(&self, sys: &mut System, ctx: HandlerCtx) {
        match self {
            Self::Basic(f) => f(sys),
            Self::Full(f) => f(sys, ctx),
        }
    }
}

pub struct ScheduledEvent {
    pub cycle: u64,
    pub handler: Handler,
}

pub struct Scheduler {
    elapsed: u64,
    soonest: u64,
    scheduled: VecDeque<ScheduledEvent>,
}

impl std::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scheduler")
            .field("elapsed", &self.elapsed)
            .field("scheduled", &self.scheduled.len())
            .finish()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            elapsed: 0,
            soonest: u64::MAX,
            scheduled: VecDeque::with_capacity(16),
        }
    }
}

impl Scheduler {
    #[inline(always)]
    fn soonest(&self) -> u64 {
        self.scheduled.front().map(|e| e.cycle).unwrap_or(u64::MAX)
    }

    #[inline(always)]
    pub fn schedule(&mut self, after: u64, handler: BasicHandler) {
        let cycle = self.elapsed + after;
        self.soonest = self.soonest.min(cycle);

        let index = self.scheduled.partition_point(|e| e.cycle <= cycle);
        self.scheduled.insert(
            index,
            ScheduledEvent {
                cycle,
                handler: Handler::Basic(handler),
            },
        );
    }

    #[inline(always)]
    pub fn schedule_now(&mut self, handler: BasicHandler) {
        self.schedule(0, handler)
    }

    #[inline(always)]
    pub fn schedule_full(&mut self, after: u64, handler: FullHandler) {
        let cycle = self.elapsed + after;
        self.soonest = self.soonest.min(cycle);

        let index = self.scheduled.partition_point(|e| e.cycle <= cycle);
        self.scheduled.insert(
            index,
            ScheduledEvent {
                cycle,
                handler: Handler::Full(handler),
            },
        );
    }

    #[inline(always)]
    pub fn cancel(&mut self, handler: BasicHandler) {
        let handler = Handler::Basic(handler);
        self.scheduled.retain(|e| e.handler != handler);
        self.soonest = self.soonest();
    }

    #[inline(always)]
    pub fn cancel_full(&mut self, handler: FullHandler) {
        let handler = Handler::Full(handler);
        self.scheduled.retain(|e| e.handler != handler);
        self.soonest = self.soonest();
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.scheduled.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn advance(&mut self, count: u64) {
        self.elapsed += count;
    }

    #[inline(always)]
    pub fn until_next(&self) -> Option<u64> {
        self.scheduled
            .front()
            .map(|e| e.cycle.saturating_sub(self.elapsed))
    }

    #[inline(always)]
    pub fn has_pending(&self) -> bool {
        self.soonest <= self.elapsed
    }

    #[inline(always)]
    pub fn pop(&mut self) -> Option<ScheduledEvent> {
        self.scheduled
            .pop_front_if(|e| e.cycle <= self.elapsed)
            .inspect(|_| self.soonest = self.soonest())
    }

    #[inline(always)]
    pub fn contains(&self, handler: BasicHandler) -> bool {
        let handler = Handler::Basic(handler);
        self.scheduled.iter().any(|e| e.handler == handler)
    }

    #[inline(always)]
    pub fn contains_full(&self, handler: FullHandler) -> bool {
        let handler = Handler::Full(handler);
        self.scheduled.iter().any(|e| e.handler == handler)
    }

    /// How many CPU cycles have elapsed.
    #[inline(always)]
    pub fn elapsed(&self) -> u64 {
        self.elapsed
    }

    /// How many time base cycles have elapsed.
    #[inline(always)]
    pub fn elapsed_time_base(&self) -> u64 {
        self.elapsed / 12
    }
}
