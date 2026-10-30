//! Sizes worked out from the containing block: `calc(100% - 24px)`,
//! `min(320px, 100%)`, `max(...)`, `clamp(240px, 50%, 480px)`. A bare number
//! is points, as everywhere in zgx. Parsed as CSS parses them, with its rule
//! on what may multiply: an expression it would reject is an error here, not
//! a layout that silently differs.
use std::{fmt, str::FromStr, sync::Arc};

/// A size to work out once the containing block's is known.
#[repr(C)]
pub struct SizeCalc {
    // First: how layout reaches it from the pointer it holds.
    head: taffy::style::CalcValue,
    // The last is the whole. Flat, not boxes: a recursive drop could unwind
    // as far as the compiler knows, and every style's drop would pay for it.
    exprs: Vec<Expr>,
}
/// A part of an expression, naming others by where they lie in `exprs`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Expr {
    Points(f32),
    /// A fraction of the containing block: `1.0` is 100%.
    Percent(f32),
    Sum(u32, u32),
    Scale(u32, f32),
    Min(u32, u32),
    Max(u32, u32),
}
fn work_out(exprs: &[Expr], at: u32, basis: f32) -> f32 {
    let of = |at| work_out(exprs, at, basis);
    match exprs[at as usize] {
        Expr::Points(value) => value,
        Expr::Percent(share) => share * basis,
        Expr::Sum(a, b) => of(a) + of(b),
        Expr::Scale(value, by) => of(value) * by,
        Expr::Min(a, b) => of(a).min(of(b)),
        Expr::Max(a, b) => of(a).max(of(b)),
    }
}
/// # Safety
/// `calc` is the address of a live `SizeCalc`, as `Scene` keeps every one a
/// laid out style names (`ColdMaps::size_calcs`).
unsafe fn resolve(calc: *const (), basis: f32) -> f32 {
    // SAFETY: as this function asks.
    let calc = unsafe { &*calc.cast::<SizeCalc>() };
    calc.resolve(basis)
}
impl SizeCalc {
    fn new(exprs: Vec<Expr>) -> Self {
        Self {
            head: taffy::style::CalcValue { resolve },
            exprs,
        }
    }
    /// The size in a containing block `basis` long: a box has no negative
    /// size, as CSS clamps `calc()` to the range a property takes.
    pub fn resolve(&self, basis: f32) -> f32 {
        let value = work_out(&self.exprs, self.exprs.len() as u32 - 1, basis);
        if value.is_finite() { value.max(0.) } else { 0. }
    }
}
impl PartialEq for SizeCalc {
    fn eq(&self, other: &Self) -> bool {
        self.exprs == other.exprs
    }
}
impl fmt::Debug for SizeCalc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.exprs.fmt(f)
    }
}
/// What of a view's size is worked out from its containing block.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SizeCalcs {
    pub(crate) width: Option<Arc<SizeCalc>>,
    pub(crate) height: Option<Arc<SizeCalc>>,
    pub(crate) min_width: Option<Arc<SizeCalc>>,
    pub(crate) max_width: Option<Arc<SizeCalc>>,
    pub(crate) min_height: Option<Arc<SizeCalc>>,
    pub(crate) max_height: Option<Arc<SizeCalc>>,
}
impl SizeCalcs {
    /// Take each size `other` sets, keeping the rest.
    pub(crate) fn merge(&mut self, other: &Self) {
        macro_rules! merge {
            ($($field:ident),*) => {$(
                if other.$field.is_some() {
                    self.$field = other.$field.clone();
                }
            )*};
        }
        merge!(width, height, min_width, max_width, min_height, max_height);
    }
}
impl FromStr for SizeCalc {
    type Err = String;
    fn from_str(written: &str) -> Result<Self, String> {
        let mut parser = Parser {
            rest: written.trim(),
            exprs: Vec::new(),
        };
        let (whole, _) = parser.function()?;
        // Last, as `SizeCalc` has it: `calc((…))` ends in what it wraps.
        let whole = parser.exprs[whole as usize];
        parser.exprs.push(whole);
        match parser.rest.trim_start() {
            "" => Ok(Self::new(parser.exprs)),
            rest => Err(format!("`{rest}` after the end of `{written}`")),
        }
    }
}
/// Where a value lies in `exprs`, and whether it's a bare number: of two
/// factors, CSS needs one to be, and a divisor always.
type Term = (u32, bool);
struct Parser<'a> {
    rest: &'a str,
    exprs: Vec<Expr>,
}
impl Parser<'_> {
    fn add(&mut self, expr: Expr) -> u32 {
        self.exprs.push(expr);
        self.exprs.len() as u32 - 1
    }
    /// What `term` is if it's a bare number.
    fn bare(&self, (at, bare): Term) -> Option<f32> {
        bare.then(|| work_out(&self.exprs, at, 0.))
    }
    fn eat(&mut self, what: char) -> bool {
        self.rest = self.rest.trim_start();
        match self.rest.strip_prefix(what) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }
    /// `calc(…)`, `min(…)`, `max(…)` or `clamp(…)`.
    fn function(&mut self) -> Result<Term, String> {
        let name = self.rest.trim_start();
        let name = &name[..name.find('(').unwrap_or(0)];
        match name.trim().to_ascii_lowercase().as_str() {
            "calc" | "min" | "max" | "clamp" => {}
            _ => {
                return Err(format!(
                    "`{}`: expected calc(…), min(…), max(…) or clamp(…)",
                    self.rest.trim()
                ));
            }
        }
        self.rest = self.rest.trim_start()[name.len()..].trim_start();
        self.eat('(');
        let mut values = vec![self.sum()?.0];
        while self.eat(',') {
            values.push(self.sum()?.0);
        }
        if !self.eat(')') {
            return Err(format!("no `)` closing {}(…)", name.trim()));
        }
        let lower = name.trim().to_ascii_lowercase();
        let fold = |parser: &mut Self, expr: fn(u32, u32) -> Expr| {
            let mut values = values.iter().copied();
            let first = values.next().unwrap_or(0);
            values.fold(first, |a, b| parser.add(expr(a, b)))
        };
        let at = match (lower.as_str(), values.len()) {
            ("calc", 1) => values[0],
            ("calc", _) => return Err("calc(…) takes one expression".into()),
            ("min", _) => fold(self, Expr::Min),
            ("max", _) => fold(self, Expr::Max),
            ("clamp", 3) => {
                let most = self.add(Expr::Min(values[1], values[2]));
                self.add(Expr::Max(values[0], most))
            }
            _ => return Err("clamp(…) takes a least, a value and a most".into()),
        };
        Ok((at, false))
    }
    fn sum(&mut self) -> Result<Term, String> {
        let mut total = self.product()?;
        loop {
            let negate = if self.eat('+') {
                false
            } else if self.eat('-') {
                true
            } else {
                return Ok(total);
            };
            let (value, bare) = self.product()?;
            let value = match negate {
                true => self.add(Expr::Scale(value, -1.)),
                false => value,
            };
            total = (self.add(Expr::Sum(total.0, value)), total.1 && bare);
        }
    }
    fn product(&mut self) -> Result<Term, String> {
        let mut total = self.value()?;
        loop {
            if self.eat('*') {
                let factor = self.value()?;
                let (value, by) = match (self.bare(total), self.bare(factor)) {
                    (_, Some(by)) => (total, by),
                    (Some(by), None) => (factor, by),
                    (None, None) => {
                        return Err("of two factors, one must be a plain number".into());
                    }
                };
                total = (self.add(Expr::Scale(value.0, by)), value.1);
            } else if self.eat('/') {
                let divisor = self.value()?;
                let Some(by) = self.bare(divisor) else {
                    return Err("a divisor must be a plain number".into());
                };
                if by == 0. {
                    return Err("division by zero".into());
                }
                total = (self.add(Expr::Scale(total.0, 1. / by)), total.1);
            } else {
                return Ok(total);
            }
        }
    }
    fn value(&mut self) -> Result<Term, String> {
        self.rest = self.rest.trim_start();
        if self.eat('(') {
            let inner = self.sum()?;
            if !self.eat(')') {
                return Err("no `)` closing a `(`".into());
            }
            return Ok(inner);
        }
        if self.rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return self.function();
        }
        let end = self
            .rest
            .char_indices()
            .find(|(at, c)| {
                !(c.is_ascii_digit() || *c == '.' || (*at == 0 && matches!(c, '-' | '+')))
            })
            .map_or(self.rest.len(), |(at, _)| at);
        let (number, rest) = self.rest.split_at(end);
        let value: f32 = number
            .parse()
            .map_err(|_| format!("expected a size at `{}`", self.rest))?;
        if !value.is_finite() {
            return Err(format!("`{number}` is out of range"));
        }
        if let Some(rest) = rest.strip_prefix('%') {
            self.rest = rest;
            return Ok((self.add(Expr::Percent(value / 100.)), false));
        }
        if let Some(rest) = rest.strip_prefix("px") {
            self.rest = rest;
            return Ok((self.add(Expr::Points(value)), false));
        }
        self.rest = rest;
        Ok((self.add(Expr::Points(value)), true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(written: &str, basis: f32) -> f32 {
        written.parse::<SizeCalc>().unwrap().resolve(basis)
    }
    #[test]
    fn expressions_work_out_as_css_does() {
        assert_eq!(at("calc(100% - 24px)", 200.), 176.);
        assert_eq!(at("calc(100% - 24)", 200.), 176.);
        assert_eq!(at("calc(50% + 10px * 2)", 200.), 120.);
        assert_eq!(at("calc((100% - 20px) / 2)", 200.), 90.);
        assert_eq!(at("calc(2 * 25%)", 200.), 100.);
        assert_eq!(at("calc(2 * 3 * 10%)", 200.), 120.);
        assert_eq!(at("calc((1 + 1) * 10px)", 200.), 20.);
        assert_eq!(at("min(320px, 100%)", 200.), 200.);
        assert_eq!(at("min(320px, 100%)", 500.), 320.);
        assert_eq!(at("max(320px, 50%)", 1000.), 500.);
        assert_eq!(at("clamp(100px, 50%, 300px)", 100.), 100.);
        assert_eq!(at("clamp(100px, 50%, 300px)", 400.), 200.);
        assert_eq!(at("clamp(100px, 50%, 300px)", 1000.), 300.);
        // CSS: the least wins over the most.
        assert_eq!(at("clamp(300px, 50%, 100px)", 400.), 300.);
        assert_eq!(at("calc(min(100%, 300px) - 20px)", 1000.), 280.);
        assert_eq!(at("CALC( 100%-24px )", 200.), 176.);
    }
    #[test]
    fn a_box_has_no_negative_size() {
        assert_eq!(at("calc(100% - 300px)", 200.), 0.);
    }
    #[test]
    fn what_css_rejects_is_an_error() {
        for written in [
            "100%",
            "calc()",
            "calc(100%",
            "calc(1px, 2px)",
            "calc(10px * 20px)",
            "calc(10% / 2px)",
            "calc(10px / 0)",
            "clamp(1px, 2px)",
            "min(",
            "calc(100% - )",
            "calc(100%) 4",
            "calc(5em)",
            "width(10px)",
            "",
        ] {
            assert!(written.parse::<SizeCalc>().is_err(), "{written}");
        }
    }
}
