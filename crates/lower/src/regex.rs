//! T114: regex literals compiled at build time into programs for the runtime's regex interpreter
//! (`backend/src/regex.rs`, `re_exec`), as Hermes ships build-time regex bytecode. The pattern (oxc's tree) becomes a
//! [`ReNode`] tree, then a program of 16-bit words (a string constant; [`program`] describes the format). Anything
//! outside the subset (Unicode mode, backreferences, lookarounds, modifier groups, `i` where folding isn't only
//! ASCII letters) is `None` and stays in regexp.js's VM. The sets are regexp.js's (`$$reClassEscape`, `.`), so
//! both engines agree; `build --regex-vm` runs everything in the VM for comparison.

use oxc_ast::ast::{RegExpFlags, RegExpLiteral};
use oxc_regular_expression::ast::{
    BoundaryAssertionKind, CharacterClass, CharacterClassContents, CharacterClassContentsKind,
    CharacterClassEscapeKind, Disjunction, Term,
};

/// Repetition counts above this run in the VM (a count is a register; huge ones are rare).
const MAX_COUNT: u64 = 1 << 20;

/// The literal's program, or `None` for the VM.
pub fn lower(r: &RegExpLiteral) -> Option<Vec<u16>> {
    let flags = r.regex.flags;
    if flags.intersects(RegExpFlags::U | RegExpFlags::V) {
        return None;
    }
    let pattern = r.regex.pattern.pattern.as_ref()?;
    // oxc 0.151 reads Annex B's `[\c_]` as `\`, `c`, `_` and keeps `[\c1]`'s `1` unconverted (V8: U+001F,
    // U+0011): a `\c` without a letter after it stays in the VM.
    let text = r.regex.pattern.text.as_bytes();
    if text.windows(3).any(|w| w[0] == b'\\' && w[1] == b'c' && !w[2].is_ascii_alphabetic())
        || text.ends_with(b"\\c")
    {
        return None;
    }
    let mut cx = Cx {
        icase: flags.contains(RegExpFlags::I),
        dot_all: flags.contains(RegExpFlags::S),
        groups: 0,
    };
    let node = cx.disjunction(&pattern.body)?;
    program(&node, cx.groups + 1, flags.contains(RegExpFlags::M))
}

/// A regex pattern in the subset (regexp.js's AST without backreferences, lookarounds, modifiers or
/// Unicode mode); `i` is already folded into the classes.
#[derive(Debug, Clone, PartialEq)]
pub enum ReNode {
    /// A sequence (empty: matches the empty string).
    Seq(Vec<ReNode>),
    /// Alternatives, tried in order.
    Alt(Vec<ReNode>),
    /// One code unit from these sorted, merged, inclusive ranges (a character, a class, `.`, an escape).
    Class(Vec<(u16, u16)>),
    /// Capturing group `n` (1-based).
    Group(u32, Box<ReNode>),
    /// `min` to `max` (`None`: no limit) repetitions; the groups `first + 1 ..= last` are inside and
    /// reset at each iteration.
    Repeat { min: u32, max: Option<u32>, greedy: bool, first: u32, last: u32, body: Box<ReNode> },
    /// `^`
    Bol,
    /// `$`
    Eol,
    /// `\b` (`false`) and `\B` (`true`).
    WordB(bool),
}

struct Cx {
    icase: bool,
    dot_all: bool,
    /// Capturing groups so far (numbered by their opening parenthesis).
    groups: u32,
}

type Ranges = Vec<(u32, u32)>;

impl Cx {
    fn disjunction(&mut self, d: &Disjunction) -> Option<ReNode> {
        let mut alts = Vec::new();
        for a in &d.body {
            let mut seq = Vec::new();
            for t in &a.body {
                seq.push(self.term(t)?);
            }
            alts.push(if seq.len() == 1 { seq.pop().unwrap() } else { ReNode::Seq(seq) });
        }
        Some(if alts.len() == 1 { alts.pop().unwrap() } else { ReNode::Alt(alts) })
    }

    fn term(&mut self, t: &Term) -> Option<ReNode> {
        Some(match t {
            Term::BoundaryAssertion(b) => match b.kind {
                BoundaryAssertionKind::Start => ReNode::Bol,
                BoundaryAssertionKind::End => ReNode::Eol,
                BoundaryAssertionKind::Boundary => ReNode::WordB(false),
                BoundaryAssertionKind::NegativeBoundary => ReNode::WordB(true),
            },
            Term::Quantifier(q) => {
                if q.min > MAX_COUNT || q.max.is_some_and(|m| m > MAX_COUNT) {
                    return None;
                }
                let first = self.groups;
                let body = self.term(&q.body)?;
                ReNode::Repeat {
                    min: q.min as u32,
                    max: q.max.map(|m| m as u32),
                    greedy: q.greedy,
                    first,
                    last: self.groups,
                    body: Box::new(body),
                }
            }
            Term::Character(c) => self.class(vec![(c.value, c.value)], false)?,
            Term::Dot(_) => {
                if self.dot_all {
                    ReNode::Class(vec![(0, 0xffff)])
                } else {
                    // Not a line terminator.
                    self.class(vec![(10, 10), (13, 13), (0x2028, 0x2029)], true)?
                }
            }
            Term::CharacterClassEscape(e) => self.class(escape(e.kind), false)?,
            Term::CharacterClass(c) => self.char_class(c)?,
            Term::CapturingGroup(g) => {
                self.groups += 1;
                let n = self.groups;
                ReNode::Group(n, Box::new(self.disjunction(&g.body)?))
            }
            Term::IgnoreGroup(g) => {
                if g.modifiers.is_some() {
                    return None;
                }
                self.disjunction(&g.body)?
            }
            Term::LookAroundAssertion(_)
            | Term::UnicodePropertyEscape(_)
            | Term::IndexedReference(_)
            | Term::NamedReference(_) => return None,
        })
    }

    fn char_class(&mut self, c: &CharacterClass) -> Option<ReNode> {
        if c.kind != CharacterClassContentsKind::Union || c.strings {
            return None;
        }
        let mut r = Ranges::new();
        for item in &c.body {
            match item {
                CharacterClassContents::CharacterClassRange(x) => r.push((x.min.value, x.max.value)),
                CharacterClassContents::CharacterClassEscape(e) => r.extend(escape(e.kind)),
                CharacterClassContents::Character(x) => r.push((x.value, x.value)),
                _ => return None,
            }
        }
        self.class(r, c.negative)
    }

    /// One code unit from `r` (or not from `r` when `negate`), with `i` folded in.
    fn class(&self, r: Ranges, negate: bool) -> Option<ReNode> {
        if r.iter().any(|&(lo, hi)| lo > hi || hi > 0xffff) {
            return None;
        }
        let mut r = normalize(r);
        if self.icase {
            // Without `u`, Canonicalize never maps a unit ≥ 128 to one below (ES2024 22.2.2.7.3), so a
            // pattern's ASCII letters only ever equal ASCII letters of the other case. Above 127 the set (before
            // a negation: CharacterSetMatcher folds the set, then inverts) must be closed under case already:
            // only caseless units (`\s`, line terminators), or all units but caseless ones (`\S`, `\W`, `.`).
            let caseless = |v: &Ranges| {
                v.iter().filter(|&&(_, hi)| hi >= 128).all(|&(lo, hi)| {
                    let lo = lo.max(128);
                    CASELESS.iter().any(|&(a, b)| a <= lo && hi <= b)
                })
            };
            if !caseless(&r) && !caseless(&complement(&r)) {
                return None;
            }
            let mut more = Vec::new();
            for &(lo, hi) in &r {
                for (a, b, d) in [(65u32, 90u32, 32i32), (97, 122, -32)] {
                    let (x, y) = (lo.max(a), hi.min(b));
                    if x <= y {
                        more.push(((x as i32 + d) as u32, (y as i32 + d) as u32));
                    }
                }
            }
            r.extend(more);
            r = normalize(r);
        }
        if negate {
            r = complement(&r);
        }
        Some(ReNode::Class(r.into_iter().map(|(a, b)| (a as u16, b as u16)).collect()))
    }
}

/// Units above 127 with no case partner that `\S`/`\W`/`.` leave out (whitespace, line terminators).
const CASELESS: [(u32, u32); 8] = [
    (0xa0, 0xa0),
    (0x1680, 0x1680),
    (0x2000, 0x200a),
    (0x2028, 0x2029),
    (0x202f, 0x202f),
    (0x205f, 0x205f),
    (0x3000, 0x3000),
    (0xfeff, 0xfeff),
];

/// regexp.js's `$$reClassEscape` without `u`.
fn escape(k: CharacterClassEscapeKind) -> Ranges {
    let d = vec![(48, 57)];
    let w = vec![(48, 57), (65, 90), (95, 95), (97, 122)];
    let s = vec![
        (9, 13),
        (32, 32),
        (0xa0, 0xa0),
        (0x1680, 0x1680),
        (0x2000, 0x200a),
        (0x2028, 0x2029),
        (0x202f, 0x202f),
        (0x205f, 0x205f),
        (0x3000, 0x3000),
        (0xfeff, 0xfeff),
    ];
    match k {
        CharacterClassEscapeKind::D => d,
        CharacterClassEscapeKind::NegativeD => complement(&d),
        CharacterClassEscapeKind::W => w,
        CharacterClassEscapeKind::NegativeW => complement(&w),
        CharacterClassEscapeKind::S => s,
        CharacterClassEscapeKind::NegativeS => complement(&s),
    }
}

/// Sorted and merged (adjacent ranges joined).
fn normalize(mut r: Ranges) -> Ranges {
    r.sort();
    let mut out: Ranges = Vec::new();
    for (lo, hi) in r {
        match out.last_mut() {
            Some(last) if lo <= last.1 + 1 => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    out
}

/// The units 0..=0xffff not in `r` (normalized).
fn complement(r: &Ranges) -> Ranges {
    let r = normalize(r.clone());
    let mut out = Vec::new();
    let mut next = 0u32;
    for (lo, hi) in r {
        if lo > next {
            out.push((next, lo - 1));
        }
        next = hi + 1;
    }
    if next <= 0xffff {
        out.push((next, 0xffff));
    }
    out
}

// ---- programs ------------------------------------------------------------------------------------
//
// A program is 16-bit words: a header, the ops, then the classes the ops and the header name (by word index).
//
// Header:
//   [0] slots: capture slots (two per group 1..) then two per general quantifier (count, iteration start); slot j
//       is `caps[2 + j]` at run time
//   [1] capture slots (reset to -1 at each start)
//   [2] flags: 1 anchored (`^` without `m` first: only position 0), 2 `m`, 4 scan (the sets below must match)
//   [3] the first op's index
//   [4] need: units a match needs from its start for the sets (the largest set offset + 1)
//   [5] n (0..=4), then n pairs (offset, class): the unit at start + offset must be in the class
//   then L, k, and L units: a literal every match contains (L 0: none), at offset k of every match
//       (0xffff: unknown)
// Ops (`pc` is a word index):
//   MATCH; CHAR c; CLASS cls; STR n u…; BOL; EOL; WORDB negate; JMP t; SPLIT t (try the next op, t on backtrack);
//   SAVE slot (= pos); CLEAR first last (slots = -1); STAR cls min max atomic (greedy one-unit loop; max 0xffff: none;
//   atomic: never gives back); LAZY cls min max; REP_INIT r; REP_LOOP r min max greedy exit (the iteration is the
//   next op); REP_ITER r; REP_END r min loop (the empty check, then to REP_LOOP).
// A class: nwide, eight words of an ASCII bitmap (unit c is bit c & 15 of word c >> 4), then nwide pairs lo, hi
//   of the ranges above 127.

pub const OP_MATCH: u16 = 0;
pub const OP_CHAR: u16 = 1;
pub const OP_CLASS: u16 = 2;
pub const OP_STR: u16 = 3;
pub const OP_BOL: u16 = 4;
pub const OP_EOL: u16 = 5;
pub const OP_WORDB: u16 = 6;
pub const OP_JMP: u16 = 7;
pub const OP_SPLIT: u16 = 8;
pub const OP_SAVE: u16 = 9;
pub const OP_CLEAR: u16 = 10;
pub const OP_STAR: u16 = 11;
pub const OP_LAZY: u16 = 12;
pub const OP_REP_INIT: u16 = 13;
pub const OP_REP_LOOP: u16 = 14;
pub const OP_REP_ITER: u16 = 15;
pub const OP_REP_END: u16 = 16;
const NONE: u16 = 0xffff;

type Set = Vec<(u16, u16)>;

/// What can follow a node: the end of the pattern, a unit from this set (consumed before any assertion), or
/// anything.
#[derive(Clone)]
enum Follow {
    End,
    Set(Set),
    Unknown,
}

/// The units a node can start with, whether it can match the empty string, and whether no assertion comes
/// before its first unit.
struct Lead {
    set: Set,
    nullable: bool,
    clean: bool,
}

fn lead(n: &ReNode) -> Lead {
    match n {
        ReNode::Class(r) => Lead { set: r.clone(), nullable: false, clean: true },
        ReNode::Seq(v) => lead_seq(v),
        ReNode::Alt(v) => {
            let mut l = Lead { set: Vec::new(), nullable: false, clean: true };
            for a in v {
                let x = lead(a);
                l.set = union(&l.set, &x.set);
                l.nullable |= x.nullable;
                l.clean &= x.clean;
            }
            l
        }
        ReNode::Group(_, b) => lead(b),
        ReNode::Repeat { min, max, body, .. } => {
            if *max == Some(0) {
                return Lead { set: Vec::new(), nullable: true, clean: true };
            }
            let x = lead(body);
            Lead { set: x.set, nullable: *min == 0 || x.nullable, clean: x.clean }
        }
        ReNode::Bol | ReNode::Eol | ReNode::WordB(_) => Lead { set: Vec::new(), nullable: true, clean: false },
    }
}

fn lead_seq(v: &[ReNode]) -> Lead {
    let mut l = Lead { set: Vec::new(), nullable: true, clean: true };
    for e in v {
        let x = lead(e);
        l.set = union(&l.set, &x.set);
        l.clean &= x.clean;
        if !x.nullable {
            l.nullable = false;
            return l;
        }
    }
    l
}

/// What follows `v[i]` in a sequence followed by `outer`.
fn follow_in(v: &[ReNode], i: usize, outer: &Follow) -> Follow {
    let l = lead_seq(&v[i + 1..]);
    if !l.clean {
        return Follow::Unknown;
    }
    if !l.nullable {
        return Follow::Set(l.set);
    }
    match outer {
        // The rest can match nothing, then the pattern ends: it always succeeds at once.
        Follow::End => Follow::End,
        Follow::Set(o) => Follow::Set(union(&l.set, o)),
        Follow::Unknown => Follow::Unknown,
    }
}

fn union(a: &[(u16, u16)], b: &[(u16, u16)]) -> Set {
    let mut r: Set = a.iter().chain(b).copied().collect();
    r.sort();
    let mut out: Set = Vec::new();
    for (lo, hi) in r {
        match out.last_mut() {
            Some(last) if lo as u32 <= last.1 as u32 + 1 => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    out
}

fn disjoint(a: &[(u16, u16)], b: &[(u16, u16)]) -> bool {
    a.iter().all(|&(lo, hi)| b.iter().all(|&(x, y)| hi < x || y < lo))
}

fn single(r: &[(u16, u16)]) -> Option<u16> {
    (r.len() == 1 && r[0].0 == r[0].1).then(|| r[0].0)
}

/// The longest run of single units every match contains, in order (at least two units), from the top-level
/// sequence and the groups in it.
fn required(n: &ReNode) -> Vec<u16> {
    fn flat(n: &ReNode, out: &mut Vec<Option<u16>>) {
        match n {
            ReNode::Seq(v) => v.iter().for_each(|e| flat(e, out)),
            ReNode::Group(_, b) => flat(b, out),
            ReNode::Class(r) if single(r).is_some() => out.push(single(r)),
            // A loop that runs at least once contributes its unit, but no run across it.
            ReNode::Repeat { min, body, .. } if *min >= 1 => {
                out.push(None);
                if let ReNode::Class(r) = &**body {
                    if let Some(u) = single(r) {
                        out.push(Some(u));
                    }
                }
                out.push(None);
            }
            // Zero-width: a run goes on across it.
            ReNode::Bol | ReNode::Eol | ReNode::WordB(_) => {}
            _ => out.push(None),
        }
    }
    let mut units = Vec::new();
    flat(n, &mut units);
    let mut best: Vec<u16> = Vec::new();
    let mut run = Vec::new();
    for u in units.into_iter().chain([None]) {
        match u {
            Some(c) => run.push(c),
            None => {
                if run.len() > best.len() {
                    best = std::mem::take(&mut run);
                }
                run.clear();
            }
        }
    }
    if best.len() >= 2 { best } else { Vec::new() }
}

/// The sets of the first units every match consumes, in order (at most 8), and whether the node always
/// consumes exactly that many. Assertions consume nothing; a loop of variable length ends the list.
fn prefix(n: &ReNode) -> (Vec<Set>, bool) {
    const MAX: usize = 8;
    match n {
        ReNode::Class(r) => (vec![r.clone()], true),
        ReNode::Bol | ReNode::Eol | ReNode::WordB(_) => (Vec::new(), true),
        ReNode::Group(_, b) => prefix(b),
        ReNode::Seq(v) => {
            let mut acc = Vec::new();
            for e in v {
                let (l, fixed) = prefix(e);
                acc.extend(l);
                if acc.len() >= MAX {
                    acc.truncate(MAX);
                    return (acc, false);
                }
                if !fixed {
                    return (acc, false);
                }
            }
            (acc, true)
        }
        ReNode::Alt(v) => {
            let ls: Vec<_> = v.iter().map(prefix).collect();
            let n = ls.iter().map(|l| l.0.len()).min().unwrap_or(0);
            let fixed = ls.iter().all(|l| l.1 && l.0.len() == n);
            let sets = (0..n).map(|i| ls.iter().fold(Vec::new(), |acc, l| union(&acc, &l.0[i]))).collect();
            (sets, fixed)
        }
        ReNode::Repeat { min, max, body, .. } => {
            let (l, fixed) = prefix(body);
            if !fixed {
                return (if *min >= 1 { l } else { Vec::new() }, false);
            }
            let mut acc = Vec::new();
            for _ in 0..*min {
                if acc.len() >= MAX {
                    break;
                }
                acc.extend(l.iter().cloned());
            }
            let exact = *max == Some(*min) && acc.len() <= MAX;
            acc.truncate(MAX);
            (acc, exact)
        }
    }
}

fn starts_with_bol(n: &ReNode) -> bool {
    match n {
        ReNode::Bol => true,
        ReNode::Seq(v) => v.first().is_some_and(starts_with_bol),
        ReNode::Group(_, b) => starts_with_bol(b),
        _ => false,
    }
}

struct Emit {
    code: Vec<u16>,
    /// Classes (contents) and the code words that name them (patched to the class's index at the end).
    classes: Vec<Set>,
    refs: Vec<(usize, usize)>,
    slots: u32,
}

impl Emit {
    fn class(&mut self, r: &Set) {
        let k = match self.classes.iter().position(|c| c == r) {
            Some(k) => k,
            None => {
                self.classes.push(r.clone());
                self.classes.len() - 1
            }
        };
        self.refs.push((self.code.len(), k));
        self.code.push(0);
    }

    fn here(&self) -> u16 {
        self.code.len() as u16
    }

    fn node(&mut self, n: &ReNode, follow: &Follow) {
        match n {
            ReNode::Class(r) => match single(r) {
                Some(u) => self.code.extend([OP_CHAR, u]),
                None => {
                    self.code.push(OP_CLASS);
                    self.class(r);
                }
            },
            ReNode::Seq(v) => {
                let mut i = 0;
                while i < v.len() {
                    // A run of single units: one STR.
                    let run: Vec<u16> = v[i..]
                        .iter()
                        .map_while(|e| if let ReNode::Class(r) = e { single(r) } else { None })
                        .collect();
                    if run.len() >= 2 {
                        self.code.extend([OP_STR, run.len() as u16]);
                        self.code.extend(&run);
                        i += run.len();
                        continue;
                    }
                    let f = follow_in(v, i, follow);
                    self.node(&v[i], &f);
                    i += 1;
                }
            }
            ReNode::Alt(v) => {
                // SPLIT next; a; JMP end; next: SPLIT next2; b; JMP end; …; last; end:
                let mut ends = Vec::new();
                for (i, a) in v.iter().enumerate() {
                    let split = (i + 1 < v.len()).then(|| {
                        self.code.extend([OP_SPLIT, 0]);
                        self.code.len() - 1
                    });
                    self.node(a, follow);
                    if let Some(at) = split {
                        self.code.extend([OP_JMP, 0]);
                        ends.push(self.code.len() - 1);
                        self.code[at] = self.here();
                    }
                }
                for at in ends {
                    self.code[at] = self.here();
                }
            }
            ReNode::Group(g, b) => {
                let j = 2 * (*g as u16 - 1);
                self.code.extend([OP_SAVE, j]);
                self.node(b, follow);
                self.code.extend([OP_SAVE, j + 1]);
            }
            ReNode::Repeat { min, max, greedy, first, last, body } => {
                if *max == Some(0) {
                    return;
                }
                if *min == 1 && *max == Some(1) {
                    return self.node(body, follow);
                }
                let (min16, max16) = (*min as u16, max.map_or(NONE, |m| m as u16));
                if let ReNode::Class(r) = &**body {
                    let atomic = match follow {
                        Follow::End => true,
                        Follow::Set(f) => disjoint(r, f),
                        Follow::Unknown => false,
                    };
                    self.code.push(if *greedy { OP_STAR } else { OP_LAZY });
                    self.class(r);
                    self.code.extend([min16, max16]);
                    if *greedy {
                        self.code.push(atomic as u16);
                    }
                    return;
                }
                let r = self.slots as u16;
                self.slots += 2;
                self.code.extend([OP_REP_INIT, r]);
                let lp = self.here();
                self.code.extend([OP_REP_LOOP, r, min16, max16, *greedy as u16, 0]);
                let exit = self.code.len() - 1;
                self.code.extend([OP_REP_ITER, r]);
                if last > first {
                    self.code.extend([OP_CLEAR, 2 * *first as u16, 2 * *last as u16 - 1]);
                }
                self.node(body, &Follow::Unknown);
                self.code.extend([OP_REP_END, r, min16, lp]);
                self.code[exit] = self.here();
            }
            ReNode::Bol => self.code.push(OP_BOL),
            ReNode::Eol => self.code.push(OP_EOL),
            ReNode::WordB(negate) => self.code.extend([OP_WORDB, *negate as u16]),
        }
    }
}

/// The program for `node` (`groups`: capture groups + 1), or `None` when it doesn't fit in 16-bit words.
fn program(node: &ReNode, groups: u32, multiline: bool) -> Option<Vec<u16>> {
    let caps = 2 * (groups - 1);
    let mut e = Emit { code: Vec::new(), classes: Vec::new(), refs: Vec::new(), slots: caps };
    // The header's sets (classes too).
    let lead = lead(node);
    let anchored = !multiline && starts_with_bol(node);
    let fixed = prefix(node).0;
    let mut sets: Vec<(usize, Set)> = Vec::new();
    if !lead.nullable && !anchored {
        sets = fixed.iter().cloned().enumerate().collect();
        if sets.is_empty() {
            sets.push((0, lead.set.clone()));
        }
    }
    let need = sets.iter().map(|x| x.0).max().map_or(0, |m| m + 1);
    sets.sort_by_key(|(_, r)| r.iter().map(|&(a, b)| b as u32 - a as u32 + 1).sum::<u32>());
    sets.truncate(4);
    let req = required(node);
    let k = (0..fixed.len()).find(|&k| {
        k + req.len() <= fixed.len() && req.iter().enumerate().all(|(i, &u)| fixed[k + i] == [(u, u)])
    });
    let flags = anchored as u16 | (multiline as u16) << 1 | (!sets.is_empty() as u16) << 2;
    e.code.extend([0, caps as u16, flags, 0, need as u16, sets.len() as u16]);
    for (o, r) in &sets {
        e.code.push(*o as u16);
        e.class(r);
    }
    e.code.extend([req.len() as u16, k.map_or(NONE, |k| k as u16)]);
    e.code.extend(&req);
    e.code[3] = e.here();
    e.node(node, &Follow::End);
    e.code.push(OP_MATCH);
    e.code[0] = u16::try_from(e.slots).ok()?;
    // The classes.
    let mut at = Vec::new();
    for c in &e.classes {
        at.push(e.code.len());
        let mut bits = [0u16; 8];
        let mut wide = Vec::new();
        for &(lo, hi) in c {
            for u in lo..=hi.min(127) {
                bits[u as usize >> 4] |= 1 << (u & 15);
            }
            if hi >= 128 {
                wide.push((lo.max(128), hi));
            }
        }
        e.code.push(wide.len() as u16);
        e.code.extend(bits);
        for (lo, hi) in wide {
            e.code.extend([lo, hi]);
        }
    }
    for (word, k) in e.refs {
        e.code[word] = u16::try_from(at[k]).ok()?;
    }
    (e.code.len() < NONE as usize).then_some(e.code)
}
