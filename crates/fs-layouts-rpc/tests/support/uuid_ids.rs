//! TEST ONLY: the mapping between the row names of the memory store (`layout-000001`) and the UUID text the contract carries.
#![allow(dead_code)]

use fs_forms_layout::*;

const KINDS: [(&str, u64); 3] = [("layout", 1), ("overlay", 2), ("log", 3)];

pub fn fwd(id: &str) -> String {
    let (word, num) = id.rsplit_once('-').unwrap_or(("", id));
    let kind = KINDS
        .iter()
        .find(|(w, _)| *w == word)
        .map_or(9, |(_, k)| *k);
    let lo: u64 = num.parse().unwrap_or(0);
    let h = format!("{:016x}{:016x}", 0x0189_0000_0000_7000_u64 + kind, lo);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

pub fn back(id: &str) -> String {
    let hex: String = id.chars().filter(|c| *c != '-').collect();
    let (hi, lo) = (
        u64::from_str_radix(&hex[..16], 16).unwrap(),
        u64::from_str_radix(&hex[16..], 16).unwrap(),
    );
    let word = KINDS
        .iter()
        .find(|(_, k)| 0x0189_0000_0000_7000_u64 + *k == hi)
        .map_or("x", |(w, _)| *w);
    format!("{word}-{lo:06}")
}

pub fn lid(i: &LayoutId) -> LayoutId {
    LayoutId(ObjectId(back(&i.0.0)))
}
pub fn oid(i: &OverlayId) -> OverlayId {
    OverlayId(ObjectId(back(&i.0.0)))
}
pub fn flid(i: &LayoutId) -> LayoutId {
    LayoutId(ObjectId(fwd(&i.0.0)))
}
pub fn foid(i: &OverlayId) -> OverlayId {
    OverlayId(ObjectId(fwd(&i.0.0)))
}
pub fn fsummary(mut s: Summary) -> Summary {
    s.id = flid(&s.id);
    s
}
pub fn freport(mut r: Report) -> Report {
    r.layout = r.layout.as_ref().map(flid);
    r.conflicts = r.conflicts.into_iter().map(fconflict).collect();
    r
}
pub fn fconflict(mut c: Conflict) -> Conflict {
    c.overlay = foid(&c.overlay);
    c
}
pub fn foverlay(mut o: Overlay) -> Overlay {
    o.id = foid(&o.id);
    o.layout = flid(&o.layout);
    o
}
