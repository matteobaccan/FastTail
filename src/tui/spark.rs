// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The sparkline row of a stream window: the first plotted field (`plot`) over the time
//! its values cover, one block character per column, on the field's own scale.

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
