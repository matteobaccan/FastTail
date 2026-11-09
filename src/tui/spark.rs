// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Sparklines: the row of a stream window plotting its first field (`plot`) over the time
//! its values cover, one block character per column, on the field's own scale; and the
//! counts per time slice of the volume row and the analysis views (`counts`).

use crate::plot::Series;

const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const ASCII: [char; 8] = ['_', '.', '-', '~', '=', '+', '*', '#'];

/// A sparkline of `series` `width` columns wide: the characters (a space where a column
/// has no value) and the lowest and highest value drawn.
pub struct Spark {
    pub text: String,
    pub min: f64,
    pub max: f64,
}

/// The sparkline of `series`; none while it holds no value or `width` is 0.
pub fn spark(series: &Series, width: usize, ascii: bool) -> Option<Spark> {
    let (start, end) = series.span()?;
    if width == 0 || end <= start {
        return None;
    }
    let span = (end - start) as i128;
    let values: Vec<Option<f64>> = (0..width as i128)
        .map(|i| {
            let from = start + (span * i / width as i128) as i64;
            let to = start + (span * (i + 1) / width as i128) as i64;
            series.value_between(from, to.max(from + 1))
        })
        .collect();
    let (min, max) = values
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    let levels = if ascii { &ASCII } else { &BLOCKS };
    let text = values
        .iter()
        .map(|v| match v {
            None => ' ',
            Some(_) if max <= min => levels[3],
            Some(v) => {
                let at = ((v - min) / (max - min) * 7.0).round() as usize;
                levels[at.min(7)]
            }
        })
        .collect();
    Some(Spark { text, min, max })
}

/// Counts per time slice as `width` block characters (slices summed in groups when there
/// are more than `width`), scaled to the largest: a space where a group counts nothing.
pub fn counts(values: &[u64], width: usize, ascii: bool) -> String {
    let groups = group_counts(values, width);
    let max = groups.iter().copied().max().unwrap_or(0);
    let levels = if ascii { &ASCII } else { &BLOCKS };
    groups
        .iter()
        .map(|&v| match v {
            0 => ' ',
            v => {
                let at = ((v as f64 / max.max(1) as f64) * 8.0).ceil() as usize;
                levels[at.clamp(1, 8) - 1]
            }
        })
        .collect()
}

/// `values` summed into at most `width` groups of adjacent slices (group `g` of `k` holds
/// slices `g * n / k .. (g + 1) * n / k`).
pub fn group_counts(values: &[u64], width: usize) -> Vec<u64> {
    let n = values.len();
    let k = n.min(width);
    (0..k)
        .map(|g| values[g * n / k..(g + 1) * n / k].iter().sum())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::{PlotSpec, PlotStat};

    fn series(points: &[(i64, &str)]) -> Series {
        let mut s = Series::new(PlotSpec {
            key: "ms".into(),
            stat: PlotStat::Avg,
        });
        for &(ts, v) in points {
            s.add(ts, v);
        }
        s
    }

    #[test]
    fn counts_scale_to_the_largest_group_and_leave_empty_slices_blank() {
        assert_eq!(counts(&[0, 1, 4, 8], 4, false), " ▁▄█");
        // Eight slices in four columns: pairs are summed.
        assert_eq!(group_counts(&[1, 1, 0, 0, 2, 2, 4, 4], 4), vec![2, 0, 4, 8]);
        assert_eq!(counts(&[1, 1, 0, 0, 2, 2, 4, 4], 4, true), ". ~#");
        assert_eq!(counts(&[], 10, false), "");
    }

    #[test]
    fn values_rise_from_the_lowest_block_to_the_highest() {
        let s = series(&[(0, "10"), (1_000, "20"), (2_000, "30"), (3_000, "80")]);
        let sp = spark(&s, 4, false).unwrap();
        assert_eq!(sp.text, "▁▂▃█");
        assert_eq!((sp.min, sp.max), (10.0, 80.0));
        assert_eq!(spark(&s, 4, true).unwrap().text, "_.-#");
    }

    #[test]
    fn gaps_are_spaces_and_a_flat_series_sits_in_the_middle() {
        let s = series(&[(0, "5"), (3_000, "5")]);
        assert_eq!(spark(&s, 4, false).unwrap().text, "▄  ▄");
        assert!(spark(&series(&[]), 4, false).is_none());
        assert!(spark(&s, 0, false).is_none());
    }

    #[test]
    fn the_next_statistic_cycles() {
        assert_eq!(PlotStat::Avg.next(), PlotStat::Max);
        assert_eq!(PlotStat::P95.next(), PlotStat::Avg);
    }
}
