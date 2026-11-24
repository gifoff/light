use super::base::*;
use super::regions::is_disabled;

impl super::base::Layout {
    pub(crate) fn hit(&self, index: usize, point: Point) -> Option<&PlacedNode> {
        let node = &self.nodes[index];
        // `pointer-events="none"` — узел не «под курсором»: hit-тест его
        // пропускает (вместе с потомками), поэтому он не перехватывает мышь.
        if node.style.pointer_events == Some(false) {
            return None;
        }
        if !node.rect.contains(point) || node.clip_rect.is_some_and(|clip| !clip.contains(point)) {
            return None;
        }
        for &child in node.children.iter().rev() {
            if let Some(found) = self.hit(child, point) {
                return Some(found);
            }
        }
        if node.is_interactive() && !is_disabled(&node.kind) {
            Some(node)
        } else {
            None
        }
    }
}
