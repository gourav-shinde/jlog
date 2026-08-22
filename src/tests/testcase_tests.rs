    use super::*;

    fn tc(start: usize, end: Option<usize>) -> TestCase {
        TestCase {
            id: "T1".to_string(),
            name: "name".to_string(),
            description: "desc".to_string(),
            start_entry: start,
            end_entry: end,
        }
    }

    #[test]
    fn range_closed_is_half_open() {
        let t = tc(2, Some(5));
        assert_eq!(t.range(100), 2..5);
        assert_eq!(t.range(100).len(), 3);
    }

    #[test]
    fn range_open_uses_total() {
        let t = tc(2, None);
        assert_eq!(t.range(10), 2..10);
    }

    #[test]
    fn range_clamps_to_total() {
        let t = tc(2, Some(50));
        assert_eq!(t.range(10), 2..10);
    }

    #[test]
    fn range_start_beyond_total_is_empty() {
        let t = tc(20, Some(30));
        let r = t.range(10);
        assert!(r.is_empty());
    }

    #[test]
    fn range_never_inverts() {
        // end before start would be nonsensical; range stays non-inverted.
        let t = tc(8, Some(3));
        let r = t.range(100);
        assert!(r.start <= r.end);
        assert!(r.is_empty());
    }

    #[test]
    fn is_recording_reflects_end_entry() {
        assert!(tc(0, None).is_recording());
        assert!(!tc(0, Some(1)).is_recording());
    }
