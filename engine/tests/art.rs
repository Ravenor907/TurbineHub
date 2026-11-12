//! Where a Steam game's pictures come from, in order: the player's own grid art, Steam's library cache
//! (library_600x900, then library_capsule; header for the wide one), then Turbine Hub's copy from Steam's store.

mod support;

use std::path::Path;
use support::scratch as temp;
use turbinehub_engine::steam;

fn put(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
}

#[test]
fn the_cover_comes_from_the_first_place_that_has_it() {
    let root = temp("cover");
    let cache = root.join("turbine-art");
    let id = "489830";
    assert_eq!(steam::find_cover(&root, id, Some(&cache)), None);
    // 4. Turbine Hub's download from Steam's store.
    let cdn = steam::cdn_art_file(&cache, id, "cover");
    put(&cdn);
    assert_eq!(steam::find_cover(&root, id, Some(&cache)), Some(cdn.clone()));
    // 2b. library_capsule in a hashed folder (newer Steam).
    let capsule = root.join("appcache/librarycache/489830/1a2b3c/library_capsule.jpg");
    put(&capsule);
    assert_eq!(steam::find_cover(&root, id, Some(&cache)), Some(capsule.clone()));
    // 2a. library_600x900 before library_capsule.
    let tall = root.join("appcache/librarycache/489830/library_600x900.jpg");
    put(&tall);
    assert_eq!(steam::find_cover(&root, id, Some(&cache)), Some(tall));
    // 1. The player's own grid art comes first of all (png before jpg).
    let grid = root.join("userdata/12345/config/grid/489830p.jpg");
    put(&grid);
    assert_eq!(steam::find_cover(&root, id, Some(&cache)), Some(grid));
    let grid_png = root.join("userdata/12345/config/grid/489830p.png");
    put(&grid_png);
    assert_eq!(steam::find_cover(&root, id, Some(&cache)), Some(grid_png));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_wide_picture_has_its_own_order() {
    let root = temp("header");
    let cache = root.join("turbine-art");
    let id = "292030";
    assert_eq!(steam::find_header(&root, id, Some(&cache)), None);
    let cdn = steam::cdn_art_file(&cache, id, "header");
    put(&cdn);
    assert_eq!(steam::find_header(&root, id, Some(&cache)), Some(cdn));
    let flat = root.join("appcache/librarycache/292030_header.jpg");
    put(&flat);
    assert_eq!(steam::find_header(&root, id, Some(&cache)), Some(flat));
    // The portrait grid art (p) is not the wide picture; <appid>.png is.
    put(&root.join("userdata/1/config/grid/292030p.png"));
    assert_eq!(steam::find_header(&root, id, Some(&cache)).unwrap().file_name().unwrap(), "292030_header.jpg");
    let wide = root.join("userdata/1/config/grid/292030.png");
    put(&wide);
    assert_eq!(steam::find_header(&root, id, Some(&cache)), Some(wide));
    // Another game's art never counts.
    assert_eq!(steam::find_header(&root, "2920", Some(&cache)), None);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn steams_store_addresses_and_the_weekly_try() {
    let u = steam::cdn_art_urls("489830");
    assert_eq!(u.len(), 3);
    assert_eq!(u[0], ("cover", "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/489830/library_600x900.jpg".to_string()));
    assert_eq!(u[1].1, "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/489830/library_capsule.jpg");
    assert_eq!(u[2], ("header", "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/489830/header.jpg".to_string()));
    assert!(steam::cdn_art_urls("../etc").is_empty());
    assert!(steam::cdn_art_urls("").is_empty());
    assert!(steam::is_picture(&[0xFF, 0xD8, 0xFF, 0xDB]));
    assert!(steam::is_picture(b"\x89PNG\r\n\x1a\n...."));
    assert!(!steam::is_picture(b"<!DOCTYPE html>"));
    let cache = temp("tried");
    assert!(steam::cdn_art_due(&cache, "10", 1_000_000));
    std::fs::write(cache.join("10.tried"), "1000000").unwrap();
    assert!(!steam::cdn_art_due(&cache, "10", 1_000_000 + 3600));
    assert!(steam::cdn_art_due(&cache, "10", 1_000_000 + steam::CDN_RETRY_SECS));
    let _ = std::fs::remove_dir_all(&cache);
}
