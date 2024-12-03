use crate::Pmt;
use crate::PmtConversionError;
use seify::Range;
use seify::RangeItem;
use std::collections::HashMap;

impl From<&Range> for Pmt {
    fn from(value: &Range) -> Self {
        Pmt::VecPmt(
            value
                .items
                .iter()
                .map(|x| match x {
                    RangeItem::Interval(min, max) => Pmt::MapStrPmt(HashMap::from([
                        ("min".to_owned(), Pmt::F64(*min)),
                        ("max".to_owned(), Pmt::F64(*max)),
                    ])),
                    RangeItem::Value(v) => Pmt::F64(*v),
                    RangeItem::Step(min, max, step) => Pmt::MapStrPmt(HashMap::from([
                        ("min".to_owned(), Pmt::F64(*min)),
                        ("max".to_owned(), Pmt::F64(*max)),
                        ("step".to_owned(), Pmt::F64(*step)),
                    ])),
                })
                .collect(),
        )
    }
}

impl From<Range> for Pmt {
    fn from(value: Range) -> Self {
        Pmt::from(&value)
    }
}

impl TryFrom<&Pmt> for Range {
    type Error = PmtConversionError;

    fn try_from(value: &Pmt) -> Result<Self, Self::Error> {
        match value {
            Pmt::VecPmt(v) => {
                let items = v
                    .iter()
                    .map(|x| match x {
                        Pmt::MapStrPmt(m) => {
                            let min: f64 = m
                                .get("min")
                                .ok_or(PmtConversionError)?
                                .to_owned()
                                .try_into()?;
                            let max = m
                                .get("max")
                                .ok_or(PmtConversionError)?
                                .to_owned()
                                .try_into()?;
                            let step = m.get("step");
                            if let Some(step) = step {
                                Ok(RangeItem::Step(
                                    min,
                                    max,
                                    step.to_owned().try_into().or(Err(PmtConversionError))?,
                                ))
                            } else {
                                Ok(RangeItem::Interval(min, max))
                            }
                        }
                        Pmt::F64(v) => Ok(RangeItem::Value(*v)),
                        _ => Err(PmtConversionError),
                    })
                    .collect::<Result<Vec<RangeItem>, PmtConversionError>>()?;
                Ok(Range { items })
            }
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for Range {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<Self, Self::Error> {
        Range::try_from(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value() -> (Range, Pmt) {
        let range = Range {
            items: vec![RangeItem::Value(3.0)],
        };
        let pmt = Pmt::VecPmt(vec![Pmt::F64(3.0)]);
        (range, pmt)
    }
    fn interval() -> (Range, Pmt) {
        let range = Range {
            items: vec![RangeItem::Interval(1.0, 2.0)],
        };
        let pmt = Pmt::VecPmt(vec![Pmt::MapStrPmt(HashMap::from([
            ("min".to_owned(), Pmt::F64(1.0)),
            ("max".to_owned(), Pmt::F64(2.0)),
        ]))]);
        (range, pmt)
    }

    fn stepped() -> (Range, Pmt) {
        let range = Range {
            items: vec![RangeItem::Step(1.0, 2.0, 0.5)],
        };
        let pmt = Pmt::VecPmt(vec![Pmt::MapStrPmt(HashMap::from([
            ("min".to_owned(), Pmt::F64(1.0)),
            ("max".to_owned(), Pmt::F64(2.0)),
            ("step".to_owned(), Pmt::F64(0.5)),
        ]))]);
        (range, pmt)
    }

    #[test]
    fn from_range_with_interval() {
        let (range, expected) = interval();
        let pmt: Pmt = range.into();
        assert_eq!(pmt, expected);
    }

    #[test]
    fn try_from_pmt_with_interval() {
        let (expected, pmt) = interval();
        let range = Range::try_from(&pmt).unwrap();
        assert_eq!(range, expected);
    }

    #[test]
    fn from_range_with_value() {
        let (range, expected) = value();
        let pmt: Pmt = range.into();
        assert_eq!(pmt, expected);
    }

    #[test]
    fn try_from_pmt_with_value() {
        let (expected, pmt) = value();
        let range = Range::try_from(&pmt).unwrap();
        assert_eq!(range, expected);
    }

    #[test]
    fn from_range_with_step() {
        let (range, expected) = stepped();
        let pmt: Pmt = range.into();
        assert_eq!(pmt, expected);
    }

    #[test]
    fn try_from_pmt_with_step() {
        let (expected, pmt) = stepped();
        let range = Range::try_from(pmt).unwrap();
        assert_eq!(range, expected);
    }

    #[test]
    fn try_from_pmt_invalid() {
        let pmt = Pmt::VecPmt(vec![Pmt::String("3.0".into()), Pmt::F64(4.0)]);
        let result = Range::try_from(&pmt);
        assert!(result.is_err());
    }
}
