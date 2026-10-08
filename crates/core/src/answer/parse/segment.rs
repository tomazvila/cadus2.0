//! Juxtaposed names: `xe^x`, `πx`, `tcos(t)`, `ye^{xy}`, `TV`.
//!
//! A run of letters that no rule of the grammar reads is split into the names it
//! is built from: a function, `pi`, a spelled Greek letter, `e`, a variable of
//! the item, or a plain letter. The product is read only when the split is
//! complete and only when it follows the rules below, so a word of prose such as
//! `yes` or `none` stays outside the grammar.
//!
//! - A run of plain letters alone belongs to [`super::build::letter_run`].
//! - A letter appears once in a run: a repeated letter is a power.
//! - The constant `e` ends the run and carries a power (`xe^x`), because `e`
//!   inside a word is a letter of that word.
//! - A function takes the names after it as its argument (`cosθ` is `cos(θ)`),
//!   or the bracket after it when it ends the run (`tcos(t)`).
//! - A capital letter or a long variable name is a variable only when the item
//!   lists it (`sY` and `TV` for the variables `s`, `Y`, `T`, `V`).

use std::cell::RefCell;

use num_bigint::BigInt;

use super::build::collapse;
use super::{FUNCTIONS, GREEK_VARIABLES, Parser, RUN_LETTERS};
use crate::answer::Undecidable;
use crate::answer::ast::{Ast, Const};
use crate::answer::lexer::Tok;

/// The longest run this reading splits, in letters.
const MAX_SEGMENT_LETTERS: usize = 12;

thread_local! {
    /// The variable names of the item whose answers are read now.
    static ITEM_VARIABLES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Restores the variable names that stood before [`with_variables`].
struct Restore(Vec<String>);

impl Drop for Restore {
    fn drop(&mut self) {
        let before = std::mem::take(&mut self.0);
        ITEM_VARIABLES.with(|names| *names.borrow_mut() = before);
    }
}

/// Run `read` with `vars` as the variable names of the item.
///
/// A `function` contract calls this around the two parses of a pair, so the
/// parser knows that `TV` is the product of the item variables `T` and `V`.
pub(crate) fn with_variables<T>(vars: &[String], read: impl FnOnce() -> T) -> T {
    let before =
        ITEM_VARIABLES.with(|names| std::mem::replace(&mut *names.borrow_mut(), vars.to_vec()));
    let _restore = Restore(before);
    read()
}

/// Whether the item declares `name` as one of its variables.
pub(super) fn is_item_variable(name: &str) -> bool {
    ITEM_VARIABLES.with(|names| names.borrow().iter().any(|listed| listed == name))
}

/// One name of a split run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Piece {
    /// A one-letter variable.
    Letter(String),
    /// The constant `pi`.
    Pi,
    /// Euler's number.
    E,
    /// A spelled Greek variable.
    Greek(String),
    /// A function of the grammar.
    Func(String),
}

impl Piece {
    /// The tree of a piece that is no function.
    fn value(&self) -> Option<Ast> {
        match self {
            Self::Letter(name) | Self::Greek(name) => Some(Ast::Var(name.clone())),
            Self::Pi => Some(Ast::Const(Const::Pi)),
            Self::E => Some(Ast::Const(Const::E)),
            Self::Func(_) => None,
        }
    }
}

/// Split `name` into pieces, or return `None` when the run is not a product.
///
/// `power_follows` is true when a `^` stands right after the run.
pub(super) fn split(
    name: &str,
    extra_functions: &[&str],
    power_follows: bool,
) -> Option<Vec<Piece>> {
    // `arcsec` and `arccsc` are inverse functions, never the product `a*r*c*sec`.
    if name.starts_with("arc")
        || name.chars().count() > MAX_SEGMENT_LETTERS
        || !name.chars().all(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let item_variables = ITEM_VARIABLES.with(|names| names.borrow().clone());
    let mut pieces = Vec::new();
    if !walk(name, extra_functions, &item_variables, &mut pieces) {
        return None;
    }
    let plain_only = pieces
        .iter()
        .all(|piece| matches!(piece, Piece::Letter(letter) if is_run_letter(letter)));
    if pieces.len() < 2 || plain_only {
        return None;
    }
    let functions = pieces
        .iter()
        .filter(|piece| matches!(piece, Piece::Func(_)))
        .count();
    let repeated = pieces.iter().enumerate().any(|(at, piece)| {
        matches!(piece, Piece::Letter(_) | Piece::Greek(_)) && pieces[at + 1..].contains(piece)
    });
    let e_misplaced = pieces
        .iter()
        .enumerate()
        .any(|(at, piece)| *piece == Piece::E && (at + 1 != pieces.len() || !power_follows));
    (functions <= 1 && !repeated && !e_misplaced).then_some(pieces)
}

/// Whether the one-letter name is a plain run letter of the grammar.
fn is_run_letter(name: &str) -> bool {
    let mut letters = name.chars();
    matches!((letters.next(), letters.next()), (Some(letter), None) if RUN_LETTERS.contains(&letter))
}

/// Split `rest` into pieces, trying the longest names first.
fn walk(rest: &str, extra: &[&str], item: &[String], pieces: &mut Vec<Piece>) -> bool {
    if rest.is_empty() {
        return true;
    }
    let mut candidates: Vec<(usize, Piece)> = Vec::new();
    for function in FUNCTIONS.iter().copied().chain(extra.iter().copied()) {
        if rest.starts_with(function) {
            candidates.push((function.len(), Piece::Func(function.to_string())));
        }
    }
    if rest.starts_with("pi") {
        candidates.push((2, Piece::Pi));
    }
    for greek in GREEK_VARIABLES {
        if rest.starts_with(greek) {
            candidates.push((greek.len(), Piece::Greek(greek.to_string())));
        }
    }
    for name in item {
        if !name.is_empty()
            && rest.starts_with(name.as_str())
            && name.chars().all(|c| c.is_ascii_alphabetic())
        {
            candidates.push((name.len(), Piece::Letter(name.clone())));
        }
    }
    if rest.starts_with('e') {
        candidates.push((1, Piece::E));
    }
    if let Some(letter) = rest.chars().next()
        && RUN_LETTERS.contains(&letter)
    {
        candidates.push((1, Piece::Letter(letter.to_string())));
    }
    candidates.sort_by_key(|(width, _)| std::cmp::Reverse(*width));
    for (width, piece) in candidates {
        pieces.push(piece);
        if walk(&rest[width..], extra, item, pieces) {
            return true;
        }
        pieces.pop();
    }
    false
}

/// Two letters of different case, `Pt` or `pA`, as the product of two variables.
fn mixed_case_pair(name: &str) -> Option<Vec<Piece>> {
    let letters: Vec<char> = name.chars().collect();
    let [first, second] = letters.as_slice() else {
        return None;
    };
    let (upper, lower) = if first.is_ascii_uppercase() {
        (*first, *second)
    } else {
        (*second, *first)
    };
    (upper.is_ascii_uppercase() && RUN_LETTERS.contains(&lower) && lower != 'e').then(|| {
        vec![
            Piece::Letter(first.to_string()),
            Piece::Letter(second.to_string()),
        ]
    })
}

impl Parser<'_> {
    /// Split the name at the cursor into pieces, when it is a product of names.
    pub(super) fn peek_pieces(&self) -> Option<Vec<Piece>> {
        let Some(Tok::Ident(name)) = self.peek() else {
            return None;
        };
        if self.is_function(name) || super::build::letter_run(name, self.extra).is_some() {
            return None;
        }
        // These names have their own reading in `parse_name`.
        if matches!(name.as_str(), "pi" | "e" | "asec" | "acsc")
            || GREEK_VARIABLES.contains(&name.as_str())
        {
            return None;
        }
        // A name the item declares is one variable, whatever its letters are.
        if is_item_variable(name) {
            return None;
        }
        // `sY(s)` is a function applied to `s`, never a product.
        if self.peek_at(1) != Some(&Tok::LParen)
            && let Some(pieces) = mixed_case_pair(name)
        {
            return Some(pieces);
        }
        split(name, self.extra, self.peek_at(1) == Some(&Tok::Pow))
    }

    /// Build the product of a split run. The power binds to the last piece.
    ///
    /// The caller took the name token already.
    pub(super) fn finish_pieces(&mut self, pieces: &[Piece]) -> Result<Ast, Undecidable> {
        let mut factors: Vec<Ast> = Vec::new();
        for (at, piece) in pieces.iter().enumerate() {
            let last = at + 1 == pieces.len();
            match piece {
                Piece::Func(name) if last => {
                    factors.push(self.parse_call(name)?);
                }
                Piece::Func(name) => {
                    let argument = self.pieces_argument(&pieces[at + 1..])?;
                    factors.push(super::build::make_call(name, vec![argument]));
                    break;
                }
                other => {
                    let value = other.value().unwrap_or(Ast::Integer(BigInt::from(1)));
                    if last {
                        let value = self.apply_percent(value)?;
                        factors.push(self.apply_power(value)?);
                    } else {
                        factors.push(value);
                    }
                }
            }
        }
        Ok(collapse(factors, Ast::Mul))
    }

    /// The argument of a function that stands in front of more names.
    fn pieces_argument(&mut self, rest: &[Piece]) -> Result<Ast, Undecidable> {
        let mut factors: Vec<Ast> = rest.iter().filter_map(Piece::value).collect();
        let Some(tail) = factors.pop() else {
            return Err(Undecidable::new("a function name with no argument"));
        };
        let tail = self.apply_percent(tail)?;
        factors.push(self.apply_power(tail)?);
        Ok(collapse(factors, Ast::Mul))
    }
}
