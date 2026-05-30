    use super::*;

    #[test]
    fn new_store_is_empty() {
        let store = LogStore::new();
        assert!(store.entries.is_empty());
        assert!(store.services.is_empty());
    }

    #[test]
    fn service_names_sorted() {
        let mut store = LogStore::new();
        store.services.insert("sshd".to_string());
        store.services.insert("kernel".to_string());
        store.services.insert("nginx".to_string());
        let names = store.service_names();
        assert_eq!(names, vec!["kernel", "nginx", "sshd"]);
    }

    #[test]
    fn service_names_empty() {
        let store = LogStore::new();
        assert!(store.service_names().is_empty());
    }
