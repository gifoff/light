#[allow(clippy::module_inception)]
mod tests {
    use crate::layout::base::Rect;
    use crate::layout::coalesce_regions;

    #[test]
    fn coalesces_overlapping_regions() {
        let regions = vec![
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(5.0, 5.0, 20.0, 20.0),
            Rect::new(40.0, 40.0, 5.0, 5.0),
        ];
        let merged = coalesce_regions(regions);
        assert_eq!(merged.len(), 2);
        assert!(merged.contains(&Rect::new(0.0, 0.0, 25.0, 25.0)));
        assert!(merged.contains(&Rect::new(40.0, 40.0, 5.0, 5.0)));
    }

    #[test]
    fn coalesces_union_induced_overlaps() {
        let regions = vec![
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(20.0, 0.0, 10.0, 10.0),
            Rect::new(5.0, 5.0, 20.0, 3.0),
        ];
        let merged = coalesce_regions(regions);
        assert_eq!(merged, vec![Rect::new(0.0, 0.0, 30.0, 10.0)]);
    }

    #[test]
    fn drops_empty_regions() {
        assert!(coalesce_regions(vec![Rect::new(0.0, 0.0, 0.0, 10.0)]).is_empty());
    }
}
