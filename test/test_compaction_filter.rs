

use rocksdb::{Writable, DB, CompactionFilter, Options};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use tempdir::TempDir;

struct Filter {
    drop_called: Arc<AtomicBool>,
    filtered_kvs: Arc<RwLock<Vec<(Vec<u8>, Vec<u8>)>>>,
}

impl CompactionFilter for Filter {
    fn filter(&mut self, _: usize, key: &[u8], value: &[u8]) -> bool {
        self.filtered_kvs.write().unwrap().push((key.to_vec(), value.to_vec()));
        true
    }
}

impl Drop for Filter {
    fn drop(&mut self) {
        self.drop_called.store(true, Ordering::Relaxed);
    }
}

#[test]
fn test_compaction_filter() {
    let path = TempDir::new("_rust_rocksdb_writebacktest").expect("");
    let mut opts = Options::new();
    let drop_called = Arc::new(AtomicBool::new(false));
    let filtered_kvs = Arc::new(RwLock::new(vec![]));
    // set ignore_snapshots to false
    opts.set_compaction_filter("test",
                               false,
                               Box::new(Filter {
                                   drop_called: drop_called.clone(),
                                   filtered_kvs: filtered_kvs.clone(),
                               }))
        .unwrap();
    opts.create_if_missing(true);
    let db = DB::open(opts, path.path().to_str().unwrap()).unwrap();
    let samples = vec![
        (b"key1".to_vec(), b"value1".to_vec()),
        (b"key2".to_vec(), b"value2".to_vec()),
    ];
    for &(ref k, ref v) in &samples {
        db.put(k, v).unwrap();
        assert_eq!(v.as_slice(), &*db.get(k).unwrap().unwrap());
    }
    {
        let _snap = db.snapshot();
        // Because ignore_snapshots is false, so force compact will not effect
        // the keys written before.
        db.compact_range(Some(b"key1"), Some(b"key3"));
        for &(ref k, ref v) in &samples {
            assert_eq!(v.as_slice(), &*db.get(k).unwrap().unwrap());
        }
        assert!(filtered_kvs.read().unwrap().is_empty());
    }
    drop(db);


    // reregister with ignore_snapshots set to true
    let mut opts = Options::new();
    opts.set_compaction_filter("test",
                               true,
                               Box::new(Filter {
                                   drop_called: drop_called.clone(),
                                   filtered_kvs: filtered_kvs.clone(),
                               }))
        .unwrap();
    assert!(drop_called.load(Ordering::Relaxed));
    drop_called.store(false, Ordering::Relaxed);
    {
        let db = DB::open(opts, path.path().to_str().unwrap()).unwrap();
        let _snap = db.snapshot();
        // Because ignore_snapshots is true, so all the keys will be compacted.
        db.compact_range(Some(b"key1"), Some(b"key3"));
        for &(ref k, _) in &samples {
            assert!(db.get(k).unwrap().is_none());
        }
        assert_eq!(*filtered_kvs.read().unwrap(), samples);
    }

    assert!(drop_called.load(Ordering::Relaxed));
}
