// Network health of each speaker: ICMP echo round trips, kept over the last few checks.
use std::collections::VecDeque;
use std::net::Ipv4Addr;

use serde::Serialize;

pub const HISTORY: usize = 15;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetStats {
    // Median round trip of the replies, in ms.
    pub rtt_ms: u32,
    // Mean difference between consecutive round trips, in ms.
    pub jitter_ms: u32,
    // Share of unanswered echoes, in percent.
    pub loss_pct: u32,
    pub samples: u32,
}

// None while the device has never answered: many computers and phones drop ICMP, which says nothing about Wi-Fi.
pub fn summarize(history: &VecDeque<Option<u32>>) -> Option<NetStats> {
    let replies: Vec<u32> = history.iter().flatten().copied().collect();
    if replies.is_empty() {
        return None;
    }
    let mut sorted = replies.clone();
    sorted.sort_unstable();
    let rtt_ms = sorted[sorted.len() / 2];
    let jitter_ms = if replies.len() > 1 {
        let sum: u32 = replies.windows(2).map(|w| w[0].abs_diff(w[1])).sum();
        (sum as f32 / (replies.len() - 1) as f32).round() as u32
    } else {
        0
    };
    let lost = history.len() - replies.len();
    Some(NetStats {
        rtt_ms,
        jitter_ms,
        loss_pct: ((lost as f32 / history.len() as f32) * 100.0).round() as u32,
        samples: history.len() as u32,
    })
}

pub fn push(history: &mut VecDeque<Option<u32>>, sample: Option<u32>) {
    if history.len() >= HISTORY {
        history.pop_front();
    }
    history.push_back(sample);
}

// One echo request; Some(round trip in ms) when the device answered within `timeout_ms`.
#[cfg(windows)]
pub fn ping(ip: Ipv4Addr, timeout_ms: u32) -> Option<u32> {
    use windows::Win32::NetworkManagement::IpHelper::{IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY};
    const PAYLOAD: [u8; 32] = [0x52; 32];
    unsafe {
        let handle = IcmpCreateFile().ok()?;
        let mut reply = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + PAYLOAD.len() + 16];
        let n = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(ip.octets()),
            PAYLOAD.as_ptr().cast(),
            PAYLOAD.len() as u16,
            None,
            reply.as_mut_ptr().cast(),
            reply.len() as u32,
            timeout_ms,
        );
        let rtt = if n > 0 {
            let r = std::ptr::read_unaligned(reply.as_ptr().cast::<ICMP_ECHO_REPLY>());
            (r.Status == 0).then_some(r.RoundTripTime)
        } else {
            None
        };
        let _ = IcmpCloseHandle(handle);
        rtt
    }
}

#[cfg(not(windows))]
pub fn ping(_ip: Ipv4Addr, _timeout_ms: u32) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reports_median_jitter_and_loss() {
        let mut h = VecDeque::new();
        assert_eq!(summarize(&h), None);
        for s in [Some(4), None, Some(6), Some(30), Some(5)] {
            push(&mut h, s);
        }
        let st = summarize(&h).unwrap();
        assert_eq!((st.rtt_ms, st.loss_pct, st.samples), (6, 20, 5));
        // |4-6| + |6-30| + |30-5| = 51 over 3 steps.
        assert_eq!(st.jitter_ms, 17);
        let silent: VecDeque<Option<u32>> = [None, None, None].into_iter().collect();
        assert_eq!(summarize(&silent), None, "a device that never answers is not reported as 100 % loss");
        for _ in 0..40 {
            push(&mut h, Some(1));
        }
        assert_eq!(h.len(), HISTORY);
    }
}
