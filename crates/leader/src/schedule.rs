use solana_pubkey::Pubkey;
use std::collections::HashMap;

use crate::Error;

const NO_LEADER: u16 = u16::MAX;

#[derive(Debug)]
pub struct LeaderSchedule {
    epoch: u64,
    first_slot: u64,
    slots_in_epoch: u64,
    leaders: Vec<Pubkey>,
    by_slot: Vec<u16>,
}

impl LeaderSchedule {
    pub fn build(
        epoch: u64,
        first_slot: u64,
        slots_in_epoch: u64,
        raw: HashMap<String, Vec<usize>>,
    ) -> Result<Self, Error> {
        let mut leaders: Vec<Pubkey> = Vec::new();
        let mut index: HashMap<Pubkey, u16> = HashMap::with_capacity(raw.len());
        let mut by_slot = vec![NO_LEADER; slots_in_epoch as usize];

        for (identity, slot_indices) in raw {
            let pubkey: Pubkey = identity
                .parse()
                .map_err(|_| Error::InvalidIdentity(identity.clone()))?;

            let leader_index = match index.get(&pubkey) {
                Some(&i) => i,
                None => {
                    let i = u16::try_from(leaders.len()).map_err(|_| Error::TooManyLeaders)?;
                    if i == NO_LEADER {
                        return Err(Error::TooManyLeaders);
                    }
                    leaders.push(pubkey);
                    index.insert(pubkey, i);
                    i
                }
            };

            for slot_index in slot_indices {
                if let Some(cell) = by_slot.get_mut(slot_index) {
                    *cell = leader_index;
                }
            }
        }

        Ok(Self {
            epoch,
            first_slot,
            slots_in_epoch,
            leaders,
            by_slot,
        })
    }

    #[inline]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    #[inline]
    pub fn contains_slot(&self, slot: u64) -> bool {
        slot >= self.first_slot && slot < self.first_slot + self.slots_in_epoch
    }

    #[inline]
    pub fn leader_at(&self, slot: u64) -> Option<&Pubkey> {
        let offset = slot.checked_sub(self.first_slot)? as usize;
        let leader_index = *self.by_slot.get(offset)?;
        if leader_index == NO_LEADER {
            return None;
        }
        self.leaders.get(leader_index as usize)
    }

    pub fn next_leader_slot(
        &self,
        from_slot: u64,
        set: &std::collections::HashSet<Pubkey>,
        max_lookahead: u64,
    ) -> Option<(u64, Pubkey)> {
        let start = from_slot.max(self.first_slot);
        let epoch_end = self.first_slot + self.slots_in_epoch;
        let end = start.saturating_add(max_lookahead).min(epoch_end);

        for slot in (start + 1)..end {
            if let Some(leader) = self.leader_at(slot)
                && set.contains(leader)
            {
                return Some((slot, *leader));
            }
        }
        None
    }
}
