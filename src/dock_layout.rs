// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The dock layout as the GUI saves it in `[dock] layout` (an `egui_dock` 0.21
//! `DockState` in RON), read and written without egui, so the terminal interface can
//! show the same arrangement of windows and save its own changes back for the GUI.
//!
//! [`Layout`] mirrors the serialized types field by field (a layout the GUI wrote comes
//! back byte for byte); [`Pane`] is the tree the terminal works with: splits with a
//! direction and a fraction, and leaves holding tabs. The main surface converts both
//! ways; floating windows and the tabs a terminal cannot show (Settings, Scratchpad...)
//! are kept as they are.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::paths_equal;

/// A position in a list (`egui_dock`'s `NodeIndex`, `TabIndex`, `SurfaceIndex`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Index(pub usize);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Pos,
    pub max: Pos,
}

impl Default for Rect {
    /// egui's `Rect::NOTHING`, what a node holds before its first layout.
    fn default() -> Self {
        Self {
            min: Pos {
                x: f32::INFINITY,
                y: f32::INFINITY,
            },
            max: Pos {
                x: f32::NEG_INFINITY,
                y: f32::NEG_INFINITY,
            },
        }
    }
}

/// A tab of the GUI's dock (`ui::dock::FastTailTab`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Tab {
    LogStream(PathBuf),
    Filters,
    Highlights,
    Settings,
    FindResults,
    Scratchpad,
    Compare,
}

impl Tab {
    pub fn stream(&self) -> Option<&Path> {
        match self {
            Tab::LogStream(p) => Some(p),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Leaf {
    pub rect: Rect,
    pub viewport: Rect,
    pub tabs: Vec<Tab>,
    pub active: Index,
    pub scroll: f32,
    pub collapsed: bool,
    pub tab_bar_hidden: bool,
}

impl Default for Leaf {
    fn default() -> Self {
        Self {
            rect: Rect::default(),
            viewport: Rect::default(),
            tabs: Vec::new(),
            active: Index(0),
            scroll: 0.0,
            collapsed: false,
            tab_bar_hidden: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SplitNode {
    pub rect: Rect,
    pub fraction: f32,
    pub fully_collapsed: bool,
    pub collapsed_leaf_count: i32,
}

impl Default for SplitNode {
    fn default() -> Self {
        Self {
            rect: Rect::default(),
            fraction: 0.5,
            fully_collapsed: false,
            collapsed_leaf_count: 0,
        }
    }
}

/// A node of a surface's binary tree, stored as a heap: the children of node `i` are
/// `2i + 1` and `2i + 2`. `Horizontal` splits left | right, `Vertical` top / bottom;
/// `fraction` is the share of the first child.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Node {
    Empty,
    Leaf(Leaf),
    Vertical(SplitNode),
    Horizontal(SplitNode),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub focused_node: Option<Index>,
    pub collapsed: bool,
    pub collapsed_leaf_count: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WindowState {
    pub screen_rect: Option<Rect>,
    pub dragged: bool,
    pub next_position: Option<Pos>,
    pub next_size: Option<Pos>,
    pub expanded_height: Option<f32>,
    pub new: bool,
    pub minimized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            screen_rect: None,
            dragged: false,
            next_position: None,
            next_size: None,
            expanded_height: None,
            new: true,
            minimized: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Surface {
    Empty,
    Main(Tree),
    Window(Tree, WindowState),
}

/// The whole dock: the main surface first, then the floating windows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Layout {
    pub surfaces: Vec<Surface>,
    pub focused_surface: Option<Index>,
}

impl Layout {
    /// Reads `[dock] layout`; `None` when it is not a layout this build knows.
    pub fn parse(text: &str) -> Option<Self> {
        ron::from_str(text).ok()
    }

    /// The layout as `[dock] layout` stores it.
    pub fn to_ron(&self) -> String {
        ron::to_string(self).unwrap_or_default()
    }

    /// A layout of one main surface holding `pane`.
    pub fn from_pane(pane: &Pane) -> Self {
        Self {
            surfaces: vec![Surface::Main(pane.to_tree())],
            focused_surface: None,
        }
    }

    /// The main surface as a pane tree (`None` when it has no tab).
    pub fn main_pane(&self) -> Option<Pane> {
        self.surfaces.iter().find_map(|s| match s {
            Surface::Main(tree) => Pane::from_tree(tree),
            _ => None,
        })
    }

    /// Replaces the main surface with `pane`; floating windows stay as they are.
    pub fn set_main_pane(&mut self, pane: &Pane) {
        let tree = pane.to_tree();
        match self
            .surfaces
            .iter_mut()
            .find(|s| matches!(s, Surface::Main(_)))
        {
            Some(main) => *main = Surface::Main(tree),
            None => self.surfaces.insert(0, Surface::Main(tree)),
        }
    }

    /// The streams of the floating windows, in order.
    pub fn window_streams(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for s in &self.surfaces {
            if let Surface::Window(tree, _) = s {
                for n in &tree.nodes {
                    if let Node::Leaf(l) = n {
                        out.extend(
                            l.tabs
                                .iter()
                                .filter_map(|t| t.stream().map(Path::to_path_buf)),
                        );
                    }
                }
            }
        }
        out
    }

    /// Drops the streams that are not in `open` from the floating windows (the main
    /// surface is the pane's business, see `Pane::reconcile`).
    pub fn retain_window_streams(&mut self, open: &[PathBuf]) {
        for s in &mut self.surfaces {
            if let Surface::Window(tree, _) = s {
                for n in &mut tree.nodes {
                    if let Node::Leaf(l) = n {
                        l.tabs.retain(|t| {
                            t.stream()
                                .is_none_or(|p| open.iter().any(|o| paths_equal(o, p)))
                        });
                        l.active = Index(l.active.0.min(l.tabs.len().saturating_sub(1)));
                    }
                }
            }
        }
    }
}

/// Which way a split divides its area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// Side by side: the first child on the left.
    Horizontal,
    /// Stacked: the first child on top.
    Vertical,
}

/// The terminal's view of a surface: splits and leaves of tabs.
#[derive(Clone, Debug, PartialEq)]
pub enum Pane {
    Leaf {
        tabs: Vec<Tab>,
        active: usize,
    },
    Split {
        dir: Dir,
        /// Share of `first`, 0 to 1.
        fraction: f32,
        first: Box<Pane>,
        second: Box<Pane>,
    },
}

impl Pane {
    /// One leaf with the streams `paths`.
    pub fn with_streams(paths: &[PathBuf]) -> Self {
        Pane::Leaf {
            tabs: paths.iter().cloned().map(Tab::LogStream).collect(),
            active: 0,
        }
    }

    /// The tree of `tree` from its root; empty leaves and splits left with one child
    /// collapse, as the GUI shows them.
    pub fn from_tree(tree: &Tree) -> Option<Self> {
        fn at(nodes: &[Node], i: usize) -> Option<Pane> {
            match nodes.get(i)? {
                Node::Empty => None,
                Node::Leaf(l) if l.tabs.is_empty() => None,
                Node::Leaf(l) => Some(Pane::Leaf {
                    tabs: l.tabs.clone(),
                    active: l.active.0.min(l.tabs.len() - 1),
                }),
                Node::Horizontal(s) | Node::Vertical(s) => {
                    let dir = if matches!(nodes[i], Node::Horizontal(_)) {
                        Dir::Horizontal
                    } else {
                        Dir::Vertical
                    };
                    match (at(nodes, 2 * i + 1), at(nodes, 2 * i + 2)) {
                        (Some(a), Some(b)) => Some(Pane::Split {
                            dir,
                            fraction: s.fraction.clamp(0.05, 0.95),
                            first: Box::new(a),
                            second: Box::new(b),
                        }),
                        (Some(one), None) | (None, Some(one)) => Some(one),
                        (None, None) => None,
                    }
                }
            }
        }
        at(&tree.nodes, 0)
    }

    /// The heap-ordered tree the GUI stores; rectangles are left for the GUI to lay out.
    pub fn to_tree(&self) -> Tree {
        fn put(nodes: &mut Vec<Node>, i: usize, pane: &Pane) {
            if nodes.len() <= i {
                nodes.resize(i + 1, Node::Empty);
            }
            match pane {
                Pane::Leaf { tabs, active } => {
                    nodes[i] = Node::Leaf(Leaf {
                        tabs: tabs.clone(),
                        active: Index(*active),
                        ..Leaf::default()
                    })
                }
                Pane::Split {
                    dir,
                    fraction,
                    first,
                    second,
                } => {
                    let split = SplitNode {
                        fraction: *fraction,
                        ..SplitNode::default()
                    };
                    nodes[i] = match dir {
                        Dir::Horizontal => Node::Horizontal(split),
                        Dir::Vertical => Node::Vertical(split),
                    };
                    put(nodes, 2 * i + 1, first);
                    put(nodes, 2 * i + 2, second);
                }
            }
        }
        let mut nodes = Vec::new();
        put(&mut nodes, 0, self);
        Tree {
            nodes,
            ..Tree::default()
        }
    }

    /// The streams of the tree, left to right and top to bottom.
    pub fn streams(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        self.walk(&mut |tabs, _| {
            out.extend(
                tabs.iter()
                    .filter_map(|t| t.stream().map(Path::to_path_buf)),
            )
        });
        out
    }

    fn walk(&self, f: &mut impl FnMut(&[Tab], usize)) {
        match self {
            Pane::Leaf { tabs, active } => f(tabs, *active),
            Pane::Split { first, second, .. } => {
                first.walk(f);
                second.walk(f);
            }
        }
    }

    /// Keeps the tree in step with the open streams, as the GUI does on restore: the
    /// streams no longer open go, the open ones missing from it (and not in a floating
    /// window) join the first leaf, and leaves left empty collapse. `None` when nothing
    /// is left.
    pub fn reconcile(self, open: &[PathBuf], elsewhere: &[PathBuf]) -> Option<Pane> {
        let is_open = |p: &Path| open.iter().any(|o| paths_equal(o, p));
        fn prune(pane: Pane, keep: &dyn Fn(&Tab) -> bool) -> Option<Pane> {
            match pane {
                Pane::Leaf { tabs, active } => {
                    let tabs: Vec<Tab> = tabs.into_iter().filter(|t| keep(t)).collect();
                    (!tabs.is_empty()).then(|| Pane::Leaf {
                        active: active.min(tabs.len() - 1),
                        tabs,
                    })
                }
                Pane::Split {
                    dir,
                    fraction,
                    first,
                    second,
                } => match (prune(*first, keep), prune(*second, keep)) {
                    (Some(a), Some(b)) => Some(Pane::Split {
                        dir,
                        fraction,
                        first: Box::new(a),
                        second: Box::new(b),
                    }),
                    (Some(one), None) | (None, Some(one)) => Some(one),
                    (None, None) => None,
                },
            }
        }
        let keep = |t: &Tab| t.stream().is_none_or(is_open);
        let pruned = prune(self, &keep);
        let present = pruned.as_ref().map(Pane::streams).unwrap_or_default();
        let missing: Vec<PathBuf> = open
            .iter()
            .filter(|o| !present.iter().chain(elsewhere).any(|p| paths_equal(p, o)))
            .cloned()
            .collect();
        match pruned {
            None if missing.is_empty() => None,
            None => Some(Pane::with_streams(&missing)),
            Some(mut pane) => {
                if let Some(Pane::Leaf { tabs, .. }) = pane.first_leaf_mut() {
                    tabs.extend(missing.into_iter().map(Tab::LogStream));
                }
                Some(pane)
            }
        }
    }

    fn first_leaf_mut(&mut self) -> Option<&mut Pane> {
        match self {
            Pane::Leaf { .. } => Some(self),
            Pane::Split { first, .. } => first.first_leaf_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// What the GUI writes for a.log | (b.log, c.log over Settings), with d.log in a
    /// floating window.
    const GUI: &str = "(surfaces:[Main((nodes:[Horizontal((rect:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),fraction:0.4,fully_collapsed:false,collapsed_leaf_count:0)),Leaf((rect:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),viewport:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),tabs:[LogStream(\"a.log\")],active:(0),scroll:0.0,collapsed:false,tab_bar_hidden:false)),Vertical((rect:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),fraction:0.7,fully_collapsed:false,collapsed_leaf_count:0)),Empty,Empty,Leaf((rect:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),viewport:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),tabs:[LogStream(\"b.log\"),LogStream(\"c.log\")],active:(0),scroll:0.0,collapsed:false,tab_bar_hidden:false)),Leaf((rect:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),viewport:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),tabs:[Settings],active:(0),scroll:0.0,collapsed:false,tab_bar_hidden:false))],focused_node:Some((6)),collapsed:false,collapsed_leaf_count:0)),Window((nodes:[Leaf((rect:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),viewport:(min:(x:inf,y:inf),max:(x:-inf,y:-inf)),tabs:[LogStream(\"d.log\")],active:(0),scroll:0.0,collapsed:false,tab_bar_hidden:false))],focused_node:None,collapsed:false,collapsed_leaf_count:0),(screen_rect:None,dragged:false,next_position:None,next_size:None,expanded_height:None,new:true,minimized:false))],focused_surface:None)";

    #[test]
    fn a_gui_layout_reads_and_writes_back_byte_for_byte() {
        let layout = Layout::parse(GUI).expect("the GUI's layout");
        assert_eq!(layout.to_ron(), GUI);
        assert_eq!(layout.window_streams(), vec![p("d.log")]);
    }

    #[test]
    fn the_main_surface_becomes_a_pane_tree_and_back() {
        let layout = Layout::parse(GUI).unwrap();
        let pane = layout.main_pane().unwrap();
        let Pane::Split {
            dir,
            fraction,
            first,
            second,
        } = &pane
        else {
            panic!("a split: {pane:?}");
        };
        assert_eq!((*dir, *fraction), (Dir::Horizontal, 0.4));
        assert_eq!(**first, Pane::with_streams(&[p("a.log")]));
        assert!(matches!(
            **second,
            Pane::Split {
                dir: Dir::Vertical,
                ..
            }
        ));
        assert_eq!(pane.streams(), vec![p("a.log"), p("b.log"), p("c.log")]);

        // Back into the layout: the same tree, the floating window untouched.
        let mut again = layout.clone();
        again.set_main_pane(&pane);
        assert_eq!(again.main_pane().unwrap(), pane);
        assert_eq!(again.surfaces[1], layout.surfaces[1]);
        assert!(Layout::parse(&again.to_ron()).is_some());
    }

    #[test]
    fn reconcile_drops_closed_streams_and_adds_new_ones() {
        let pane = Layout::parse(GUI).unwrap().main_pane().unwrap();
        // c.log closed, e.log opened; d.log lives in a floating window.
        let open = [p("a.log"), p("b.log"), p("d.log"), p("e.log")];
        let pane = pane.reconcile(&open, &[p("d.log")]).unwrap();
        assert_eq!(pane.streams(), vec![p("a.log"), p("e.log"), p("b.log")]);
        // Everything closed but a new one: one leaf with it.
        let pane = Pane::with_streams(&[p("a.log")])
            .reconcile(&[p("z.log")], &[])
            .unwrap();
        assert_eq!(pane, Pane::with_streams(&[p("z.log")]));
        assert!(Pane::with_streams(&[p("a.log")])
            .reconcile(&[], &[])
            .is_none());
    }

    #[test]
    fn unknown_text_is_not_a_layout() {
        assert!(Layout::parse("not ron").is_none());
        assert!(Layout::parse("(surfaces:[Main((nodes:[Leaf((tabs:[Nope]))]))])").is_none());
    }

    #[cfg(feature = "gui")]
    #[test]
    fn the_guis_own_dock_state_round_trips() {
        use crate::ui::dock::FastTailTab;
        use egui_dock::{DockState, NodeIndex};
        let mut ds = DockState::new(vec![FastTailTab::LogStream("a.log".into())]);
        let [_, b] = ds.main_surface_mut().split_below(
            NodeIndex::root(),
            0.25,
            vec![
                FastTailTab::LogStream("b.log".into()),
                FastTailTab::Scratchpad,
            ],
        );
        ds.main_surface_mut()
            .split_right(b, 0.6, vec![FastTailTab::Highlights]);
        let gui = ron::to_string(&ds).unwrap();
        let layout = Layout::parse(&gui).expect("parsed");
        assert_eq!(layout.to_ron(), gui, "byte for byte");
        // A tree written by the terminal is one the GUI reads.
        let mut changed = layout.clone();
        changed.set_main_pane(&Pane::with_streams(&[p("x.log")]));
        let back: DockState<FastTailTab> = ron::from_str(&changed.to_ron()).expect("GUI reads it");
        assert_eq!(back.iter_all_tabs().count(), 1);
    }
}
