use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The leaf content of a pane: a terminal session, an open file, or a git diff.
///
/// Each variant carries the *id* of the managed resource so the frontend can
/// look up its own rendered state without the backend needing to ship the
/// full payload over the FFI boundary on every render.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum PaneContent {
    Session(Uuid),
    File(Uuid),
    Diff(Uuid),
}

impl PaneContent {
    pub fn is_diff(&self) -> bool {
        matches!(self, PaneContent::Diff(_))
    }
}

/// Which side of a target pane a dragged pane is dropped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaneDropEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// One tile in a tab's layout. A value type so structural changes reassign the
/// enclosing column array and the frontend re-renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: Uuid,
    pub content: PaneContent,
    /// Relative vertical share within its column.
    pub weight: f32,
}

impl Pane {
    pub fn new(content: PaneContent) -> Self {
        Self { id: Uuid::new_v4(), content, weight: 1.0 }
    }
}

/// A vertical stack of panes; columns tile left-to-right across the tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneColumn {
    pub id: Uuid,
    pub panes: Vec<Pane>,
    /// Relative horizontal share within the tab.
    pub weight: f32,
}

impl PaneColumn {
    pub fn one(pane: Pane) -> Self {
        Self { id: Uuid::new_v4(), panes: vec![pane], weight: 1.0 }
    }
}

/// One entry in a project's tab strip: a niri-style layout of panes arranged
/// as a row of columns, each column a vertical stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneTab {
    pub id: Uuid,
    pub custom_name: Option<String>,
    pub columns: Vec<PaneColumn>,
    pub focused_pane_id: Uuid,
    pub is_zoomed: bool,
}

impl PaneTab {
    pub fn new(content: PaneContent) -> Self {
        let pane = Pane::new(content);
        let id = pane.id;
        Self {
            id: Uuid::new_v4(),
            custom_name: None,
            columns: vec![PaneColumn::one(pane)],
            focused_pane_id: id,
            is_zoomed: false,
        }
    }

    pub fn all_panes(&self) -> Vec<&Pane> {
        self.columns.iter().flat_map(|c| c.panes.iter()).collect()
    }

    pub fn has_multiple_panes(&self) -> bool {
        self.all_panes().len() > 1
    }

    /// Detach (remove) a pane from this tab and return it by value so a caller
    /// can re-insert it into another tab — the tab's last pane may be taken.
    /// The source column is dropped if it becomes empty. The tab's focused
    /// pane id is repointed to another pane in this tab (or left as-is if no
    /// pane remains). No-op (returns None) when the pane is not present. A
    /// caller that empties a tab is expected to close it afterwards (see
    /// AppState::move_pane_cross_tab).
    pub fn detach_pane_allowing_empty(&mut self, pane_id: Uuid) -> Option<Pane> {
        let (ci, ri) = self.columns.iter().enumerate().find_map(|(ci, col)| {
            col.panes.iter().position(|p| p.id == pane_id).map(|ri| (ci, ri))
        })?;
        let pane = self.columns[ci].panes.remove(ri);
        if self.columns[ci].panes.is_empty() {
            self.columns.remove(ci);
        }
        // Adjust weights so columns sum to 1.0 after the drop.
        let total: f32 = self.columns.iter().map(|c| c.weight).sum();
        if total > 0.0 {
            for c in &mut self.columns {
                c.weight /= total;
            }
        }
        // Focus the first remaining pane if we detached the focused one.
        if self.focused_pane_id == pane_id {
            self.focused_pane_id = self.all_panes().first().map(|p| p.id).unwrap_or(pane_id);
        }
        self.is_zoomed = false;
        Some(pane)
    }

    /// Add `pane` as a new single-pane column in this tab (so dropped panes
    /// arriving from another tab get their own column), and focus it.
    pub fn add_pane_as_column(&mut self, pane: Pane) {
        self.is_zoomed = false;
        let new_id = pane.id;
        // Halve every existing column's weight so the new column takes half
        // the tab while the others keep their proportions relative to each
        // other.
        for c in &mut self.columns {
            c.weight *= 0.5;
        }
        let col = PaneColumn { id: Uuid::new_v4(), panes: vec![pane], weight: 0.5 };
        self.columns.push(col);
        self.focused_pane_id = new_id;
    }

    pub fn can_split(&self) -> bool {
        match self.focused_pane() {
            Some(p) => !p.content.is_diff(),
            None => false,
        }
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.all_panes().into_iter().find(|p| p.id == self.focused_pane_id)
    }

    pub fn focused_location(&self) -> Option<(usize, usize)> {
        for (ci, col) in self.columns.iter().enumerate() {
            if let Some(ri) = col.panes.iter().position(|p| p.id == self.focused_pane_id) {
                return Some((ci, ri));
            }
        }
        None
    }

    /// Inserts `pane` next to the focused pane on `edge`, taking half the space
    /// it splits into. Focuses the new pane.
    pub fn split(&mut self, pane: Pane, edge: PaneDropEdge) {
        self.is_zoomed = false;
        let new_pane_id = pane.id;
        let Some((col, row)) = self.focused_location() else {
            self.columns.push(PaneColumn::one(pane));
            self.focused_pane_id = new_pane_id;
            return;
        };
        match edge {
            PaneDropEdge::Left | PaneDropEdge::Right => {
                let share = self.columns[col].weight / 2.0;
                self.columns[col].weight = share;
                let mut new_col = PaneColumn::one(pane);
                new_col.weight = share;
                let insert_at = if edge == PaneDropEdge::Left { col } else { col + 1 };
                self.columns.insert(insert_at, new_col);
            }
            PaneDropEdge::Top | PaneDropEdge::Bottom => {
                let share = self.columns[col].panes[row].weight / 2.0;
                self.columns[col].panes[row].weight = share;
                let mut inserted = pane;
                inserted.weight = share;
                let insert_at = if edge == PaneDropEdge::Top { row } else { row + 1 };
                self.columns[col].panes.insert(insert_at, inserted);
            }
        }
        self.focused_pane_id = new_pane_id;
    }

    /// Move an existing pane to an edge of another pane (drag & drop
    /// rearrange within one tab). No-op when source == target or either id
    /// is missing. Unlike `split`, no space is taken from the target: the
    /// moved pane keeps its weight and the target's weights are left alone
    /// (the user can drag dividers to rebalance). A source column emptied by
    /// the move is dropped; focus follows the moved pane.
    pub fn move_pane(&mut self, pane_id: Uuid, target_pane_id: Uuid, edge: PaneDropEdge) {
        if pane_id == target_pane_id {
            return;
        }
        fn locate(columns: &[PaneColumn], id: Uuid) -> Option<(usize, usize)> {
            for (ci, col) in columns.iter().enumerate() {
                if let Some(ri) = col.panes.iter().position(|p| p.id == id) {
                    return Some((ci, ri));
                }
            }
            None
        }
        let Some((sc, sr)) = locate(&self.columns, pane_id) else { return };
        let Some((tc, tr)) = locate(&self.columns, target_pane_id) else { return };
        self.is_zoomed = false;
        let pane = self.columns[sc].panes.remove(sr);
        // Removal shifts the target's coordinates: its row when both panes
        // shared a column and the source sat above, its column when the
        // emptied source column (left of the target) is dropped below.
        let mut tc = tc;
        let mut tr = tr;
        if sc == tc && sr < tr {
            tr -= 1;
        }
        if self.columns[sc].panes.is_empty() {
            self.columns.remove(sc);
            if sc < tc {
                tc -= 1;
            }
        }
        match edge {
            PaneDropEdge::Left | PaneDropEdge::Right => {
                let insert_at = if edge == PaneDropEdge::Left { tc } else { tc + 1 };
                self.columns.insert(insert_at, PaneColumn::one(pane));
            }
            PaneDropEdge::Top | PaneDropEdge::Bottom => {
                let insert_at = if edge == PaneDropEdge::Top { tr } else { tr + 1 };
                self.columns[tc].panes.insert(insert_at, pane);
            }
        }
        self.focused_pane_id = pane_id;
    }

    /// Focus navigation within the tab.
    pub fn focus(&mut self, direction: FocusDirection) {
        self.is_zoomed = false;
        let Some((col, row)) = self.focused_location() else { return };
        match direction {
            FocusDirection::Up => {
                if row > 0 {
                    self.focused_pane_id = self.columns[col].panes[row - 1].id;
                }
            }
            FocusDirection::Down => {
                if row + 1 < self.columns[col].panes.len() {
                    self.focused_pane_id = self.columns[col].panes[row + 1].id;
                }
            }
            FocusDirection::Left => {
                if col > 0 {
                    let next = self.columns[col - 1].panes.len().saturating_sub(1).min(row);
                    self.focused_pane_id = self.columns[col - 1].panes[next].id;
                }
            }
            FocusDirection::Right => {
                if col + 1 < self.columns.len() {
                    let target_col = col + 1;
                    let next = self.columns[target_col].panes.len().saturating_sub(1).min(row);
                    self.focused_pane_id = self.columns[target_col].panes[next].id;
                }
            }
            FocusDirection::Next => self.cycle(1),
            FocusDirection::Previous => self.cycle(-1),
        }
    }

    fn cycle(&mut self, delta: i32) {
        let panes: Vec<uuid::Uuid> = self
            .columns
            .iter()
            .flat_map(|c| c.panes.iter().map(|p| p.id))
            .collect();
        if panes.len() <= 1 {
            return;
        }
        let idx = panes.iter().position(|id| *id == self.focused_pane_id).unwrap_or(0);
        let next = ((idx as i32 + delta).rem_euclid(panes.len() as i32)) as usize;
        self.focused_pane_id = panes[next];
    }

    pub fn toggle_zoom(&mut self) {
        if self.is_zoomed {
            self.is_zoomed = false;
        } else if self.has_multiple_panes() {
            self.is_zoomed = true;
        }
    }

    pub fn equalize(&mut self) {
        self.is_zoomed = false;
        for col in &mut self.columns {
            col.weight = 1.0;
            for pane in &mut col.panes {
                pane.weight = 1.0;
            }
        }
    }

    /// Resize one step. `direction` signs which way the divider moves; the
    /// divider chosen is on the pressed side, falling back at the edges.
    pub fn resize(&mut self, direction: ResizeDirection) {
        self.is_zoomed = false;
        const STEP: f32 = 0.05;
        const MIN_SHARE: f32 = 0.1;
        let Some((col, row)) = self.focused_location() else { return };
        match direction {
            ResizeDirection::Left | ResizeDirection::Right => {
                if self.columns.len() < 2 {
                    return;
                }
                let divider = match direction {
                    ResizeDirection::Right => {
                        if col + 1 < self.columns.len() { col } else { col - 1 }
                    }
                    ResizeDirection::Left => {
                        if col > 0 { col - 1 } else { col }
                    }
                    _ => col,
                };
                let total: f32 = self.columns.iter().map(|c| c.weight).sum();
                if total <= 0.0 {
                    return;
                }
                let donor_idx = if matches!(direction, ResizeDirection::Right) {
                    divider + 1
                } else {
                    divider
                };
                let donor = self.columns[donor_idx].weight;
                let step = (total * STEP).min((donor - total * MIN_SHARE).max(0.0));
                let delta = if matches!(direction, ResizeDirection::Right) { step } else { -step };
                self.columns[divider].weight += delta;
                self.columns[divider + 1].weight -= delta;
            }
            ResizeDirection::Up | ResizeDirection::Down => {
                if self.columns[col].panes.len() < 2 {
                    return;
                }
                let divider = match direction {
                    ResizeDirection::Down => {
                        if row + 1 < self.columns[col].panes.len() { row } else { row - 1 }
                    }
                    ResizeDirection::Up => {
                        if row > 0 { row - 1 } else { row }
                    }
                    _ => row,
                };
                let panes = &mut self.columns[col].panes;
                let total: f32 = panes.iter().map(|p| p.weight).sum();
                if total <= 0.0 {
                    return;
                }
                let donor_idx = if matches!(direction, ResizeDirection::Down) {
                    divider + 1
                } else {
                    divider
                };
                let donor = panes[donor_idx].weight;
                let step = (total * STEP).min((donor - total * MIN_SHARE).max(0.0));
                let delta = if matches!(direction, ResizeDirection::Down) { step } else { -step };
                panes[divider].weight += delta;
                panes[divider + 1].weight -= delta;
            }
        }
    }

    /// Precise drag-resize of one divider. `vertical` picks the axis: true is
    /// the divider between `columns[index]` and `columns[index + 1]`; false is
    /// the divider between panes `index` and `index + 1` inside
    /// `columns[column_index]`. `delta` is added to the first weight and
    /// subtracted from the second, keeping the sum constant; both weights are
    /// clamped to >= MIN_WEIGHT and the applied delta shrinks accordingly.
    pub fn resize_divider(&mut self, vertical: bool, column_index: usize, index: usize, delta: f32) {
        const MIN_WEIGHT: f32 = 0.05;
        self.is_zoomed = false;
        let pair: Option<(&mut f32, &mut f32)> = if vertical {
            if index + 1 >= self.columns.len() {
                None
            } else {
                let (left, right) = self.columns.split_at_mut(index + 1);
                Some((&mut left[index].weight, &mut right[0].weight))
            }
        } else {
            match self.columns.get_mut(column_index) {
                Some(col) if index + 1 < col.panes.len() => {
                    let (top, bottom) = col.panes.split_at_mut(index + 1);
                    Some((&mut top[index].weight, &mut bottom[0].weight))
                }
                _ => None,
            }
        };
        let Some((a, b)) = pair else { return };
        if !delta.is_finite() {
            return;
        }
        let lo = MIN_WEIGHT - *a;
        let hi = *b - MIN_WEIGHT;
        let applied = if lo > hi { 0.0 } else { delta.clamp(lo, hi) };
        *a += applied;
        *b -= applied;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusDirection {
    Up,
    Down,
    Left,
    Right,
    Next,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResizeDirection {
    Up,
    Down,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_pane() -> Pane {
        Pane::new(PaneContent::Session(Uuid::new_v4()))
    }

    fn single_tab() -> PaneTab {
        PaneTab::new(PaneContent::Session(Uuid::new_v4()))
    }

    // ---- split -------------------------------------------------------------

    #[test]
    fn split_right_halves_column_weight_and_focuses_new_pane() {
        let mut tab = single_tab();
        let original = tab.focused_pane_id;
        let new_pane = session_pane();
        let new_id = new_pane.id;

        tab.split(new_pane, PaneDropEdge::Right);

        assert_eq!(tab.columns.len(), 2);
        assert_eq!(tab.columns[0].weight, 0.5);
        assert_eq!(tab.columns[1].weight, 0.5);
        assert_eq!(tab.columns[0].panes[0].id, original);
        assert_eq!(tab.columns[1].panes[0].id, new_id);
        assert_eq!(tab.focused_pane_id, new_id);
    }

    #[test]
    fn split_left_inserts_before_focused_column() {
        let mut tab = single_tab();
        let original = tab.focused_pane_id;
        let new_pane = session_pane();
        let new_id = new_pane.id;

        tab.split(new_pane, PaneDropEdge::Left);

        assert_eq!(tab.columns.len(), 2);
        assert_eq!(tab.columns[0].panes[0].id, new_id);
        assert_eq!(tab.columns[1].panes[0].id, original);
        assert_eq!(tab.focused_pane_id, new_id);
    }

    #[test]
    fn split_bottom_halves_pane_weight_within_column() {
        let mut tab = single_tab();
        let original = tab.focused_pane_id;
        let new_pane = session_pane();
        let new_id = new_pane.id;

        tab.split(new_pane, PaneDropEdge::Bottom);

        assert_eq!(tab.columns.len(), 1);
        assert_eq!(tab.columns[0].panes.len(), 2);
        assert_eq!(tab.columns[0].panes[0].id, original);
        assert_eq!(tab.columns[0].panes[0].weight, 0.5);
        assert_eq!(tab.columns[0].panes[1].id, new_id);
        assert_eq!(tab.columns[0].panes[1].weight, 0.5);
        assert_eq!(tab.focused_pane_id, new_id);
    }

    #[test]
    fn split_top_inserts_before_focused_pane() {
        let mut tab = single_tab();
        let original = tab.focused_pane_id;
        let new_pane = session_pane();
        let new_id = new_pane.id;

        tab.split(new_pane, PaneDropEdge::Top);

        assert_eq!(tab.columns[0].panes.len(), 2);
        assert_eq!(tab.columns[0].panes[0].id, new_id);
        assert_eq!(tab.columns[0].panes[1].id, original);
        assert_eq!(tab.focused_pane_id, new_id);
    }

    #[test]
    fn split_with_unknown_focus_appends_column() {
        let mut tab = single_tab();
        tab.focused_pane_id = Uuid::new_v4(); // points at nothing
        let new_pane = session_pane();
        let new_id = new_pane.id;

        tab.split(new_pane, PaneDropEdge::Right);

        assert_eq!(tab.columns.len(), 2);
        assert_eq!(tab.columns[1].panes[0].id, new_id);
        assert_eq!(tab.focused_pane_id, new_id);
    }

    // ---- resize_divider ----------------------------------------------------

    #[test]
    fn resize_divider_keeps_sum_constant() {
        let mut tab = single_tab();
        tab.split(session_pane(), PaneDropEdge::Right);
        let before: f32 = tab.columns.iter().map(|c| c.weight).sum();

        tab.resize_divider(true, 0, 0, 0.2);

        assert_eq!(tab.columns[0].weight, 0.5 + 0.2);
        assert_eq!(tab.columns[1].weight, 0.5 - 0.2);
        let after: f32 = tab.columns.iter().map(|c| c.weight).sum();
        assert!((before - after).abs() < 1e-6);
    }

    #[test]
    fn resize_divider_clamps_to_minimum_weight() {
        let mut tab = single_tab();
        tab.split(session_pane(), PaneDropEdge::Right);

        tab.resize_divider(true, 0, 0, 10.0);
        assert!((tab.columns[0].weight - 0.95).abs() < 1e-6);
        assert!((tab.columns[1].weight - 0.05).abs() < 1e-6);

        tab.resize_divider(true, 0, 0, -10.0);
        assert!((tab.columns[0].weight - 0.05).abs() < 1e-6);
        assert!((tab.columns[1].weight - 0.95).abs() < 1e-6);
    }

    #[test]
    fn resize_divider_rejects_non_finite_delta() {
        let mut tab = single_tab();
        tab.split(session_pane(), PaneDropEdge::Right);

        for delta in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            tab.resize_divider(true, 0, 0, delta);
            assert_eq!(tab.columns[0].weight, 0.5);
            assert_eq!(tab.columns[1].weight, 0.5);
        }
    }

    #[test]
    fn resize_divider_out_of_range_is_noop() {
        let mut tab = single_tab();
        tab.split(session_pane(), PaneDropEdge::Right);

        // No divider after the last column; bogus column index for panes.
        tab.resize_divider(true, 0, 1, 0.2);
        tab.resize_divider(false, 99, 0, 0.2);

        assert_eq!(tab.columns[0].weight, 0.5);
        assert_eq!(tab.columns[1].weight, 0.5);
    }

    #[test]
    fn resize_divider_horizontal_between_panes() {
        let mut tab = single_tab();
        tab.split(session_pane(), PaneDropEdge::Bottom);

        tab.resize_divider(false, 0, 0, 0.3);

        assert_eq!(tab.columns[0].panes[0].weight, 0.5 + 0.3);
        assert_eq!(tab.columns[0].panes[1].weight, 0.5 - 0.3);
    }

    // ---- move_pane ----------------------------------------------------------

    #[test]
    fn move_pane_within_same_column_shifts_insert_index() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;
        let b = session_pane();
        let b_id = b.id;
        tab.split(b, PaneDropEdge::Bottom);
        let c = session_pane();
        let c_id = c.id;
        tab.split(c, PaneDropEdge::Bottom);
        // Column is now [a, b, c]; move a below c.
        tab.move_pane(a, c_id, PaneDropEdge::Bottom);

        let ids: Vec<Uuid> = tab.columns[0].panes.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![b_id, c_id, a]);
        assert_eq!(tab.focused_pane_id, a);
    }

    #[test]
    fn move_pane_drops_emptied_source_column() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;
        let b = session_pane();
        let b_id = b.id;
        tab.split(b, PaneDropEdge::Right);
        // columns: [a] [b]; move a to the right edge of b.
        tab.move_pane(a, b_id, PaneDropEdge::Right);

        assert_eq!(tab.columns.len(), 2);
        assert_eq!(tab.columns[0].panes[0].id, b_id);
        assert_eq!(tab.columns[1].panes[0].id, a);
        assert_eq!(tab.focused_pane_id, a);
    }

    #[test]
    fn move_pane_onto_bottom_of_other_column_keeps_target_weights() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;
        let b = session_pane();
        let b_id = b.id;
        tab.split(b, PaneDropEdge::Right);
        let b_weight = tab.columns[1].panes[0].weight;
        // Move a below b: column 0 empties and is dropped.
        tab.move_pane(a, b_id, PaneDropEdge::Bottom);

        assert_eq!(tab.columns.len(), 1);
        let ids: Vec<Uuid> = tab.columns[0].panes.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![b_id, a]);
        // Unlike split, no space is taken from the target.
        assert_eq!(tab.columns[0].panes[0].weight, b_weight);
        assert_eq!(tab.focused_pane_id, a);
    }

    #[test]
    fn move_pane_self_drop_is_noop() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;
        tab.is_zoomed = true;

        tab.move_pane(a, a, PaneDropEdge::Right);

        assert_eq!(tab.columns.len(), 1);
        assert_eq!(tab.columns[0].panes.len(), 1);
        assert_eq!(tab.focused_pane_id, a);
        // The early return happens before zoom is cleared.
        assert!(tab.is_zoomed);
    }

    #[test]
    fn move_pane_unknown_ids_is_noop() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;

        tab.move_pane(Uuid::new_v4(), a, PaneDropEdge::Right);
        tab.move_pane(a, Uuid::new_v4(), PaneDropEdge::Right);

        assert_eq!(tab.columns.len(), 1);
        assert_eq!(tab.columns[0].panes.len(), 1);
    }

    // ---- focus / cycle -----------------------------------------------------

    #[test]
    fn focus_navigates_between_columns_and_rows() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;
        let b = session_pane();
        let b_id = b.id;
        tab.split(b, PaneDropEdge::Bottom);
        let c = session_pane();
        let c_id = c.id;
        tab.split(c, PaneDropEdge::Right);
        // Layout: col0 = [a, b], col1 = [c]; focus is on c.

        tab.focus(FocusDirection::Left);
        assert_eq!(tab.focused_pane_id, a); // row 0 of the left column

        tab.focus(FocusDirection::Down);
        assert_eq!(tab.focused_pane_id, b_id);

        tab.focus(FocusDirection::Right);
        assert_eq!(tab.focused_pane_id, c_id); // row clamped to col1's only pane

        tab.focus(FocusDirection::Right); // at the right edge: stays
        assert_eq!(tab.focused_pane_id, c_id);

        tab.focus(FocusDirection::Up); // single-row column: stays
        assert_eq!(tab.focused_pane_id, c_id);
    }

    #[test]
    fn focus_cycle_wraps_in_both_directions() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;
        let b = session_pane();
        let b_id = b.id;
        tab.split(b, PaneDropEdge::Bottom);
        let c = session_pane();
        let c_id = c.id;
        tab.split(c, PaneDropEdge::Right);
        // Flattened order: [a, b, c]; focus on c.

        tab.focus(FocusDirection::Next); // wraps to first
        assert_eq!(tab.focused_pane_id, a);
        tab.focus(FocusDirection::Next);
        assert_eq!(tab.focused_pane_id, b_id);
        tab.focus(FocusDirection::Previous);
        assert_eq!(tab.focused_pane_id, a);
        tab.focus(FocusDirection::Previous); // wraps to last
        assert_eq!(tab.focused_pane_id, c_id);
    }

    #[test]
    fn focus_cycle_single_pane_stays() {
        let mut tab = single_tab();
        let a = tab.focused_pane_id;
        tab.focus(FocusDirection::Next);
        tab.focus(FocusDirection::Previous);
        assert_eq!(tab.focused_pane_id, a);
    }

    // ---- equalize -----------------------------------------------------------

    #[test]
    fn equalize_resets_all_weights_and_clears_zoom() {
        let mut tab = single_tab();
        tab.split(session_pane(), PaneDropEdge::Right);
        tab.split(session_pane(), PaneDropEdge::Bottom);
        tab.resize_divider(true, 0, 0, 0.2);
        tab.is_zoomed = true;

        tab.equalize();

        for col in &tab.columns {
            assert_eq!(col.weight, 1.0);
            for pane in &col.panes {
                assert_eq!(pane.weight, 1.0);
            }
        }
        assert!(!tab.is_zoomed);
    }
}
