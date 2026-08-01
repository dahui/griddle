//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

fn params(pairs: &[(&str, &str)]) -> FilterParams {
    let get = |k: &str| {
        pairs
            .iter()
            .find(|(key, _)| *key == k)
            .map(|(_, v)| (*v).to_owned())
    };
    FilterParams {
        styles: get("styles"),
        dimensions: get("dimensions"),
        mimes: get("mimes"),
        types: get("types"),
        nsfw: get("nsfw"),
        humor: get("humor"),
        epilepsy: get("epilepsy"),
        oneoftag: get("oneoftag"),
    }
}

#[test]
fn from_params_accepts_a_real_filter_set() {
    // The control. Every other test here asserts a *rejection*, and an all-negative suite
    // cannot tell "correctly refused" from "refuses everything" — including the values the
    // UI actually sends.
    let q = AssetQuery::from_params(
        AssetKind::Grid,
        &params(&[
            ("dimensions", "600x900,342x482"),
            ("styles", "alternate,blurred"),
            ("mimes", "image/png"),
            ("types", "static,animated"),
            ("nsfw", "false"),
            ("humor", "any"),
            ("epilepsy", "true"),
            ("oneoftag", "humor,nsfw"),
        ]),
    )
    .unwrap();

    assert_eq!(
        q.dimensions,
        vec![Dimensions::D600x900, Dimensions::D342x482]
    );
    assert_eq!(q.styles, vec!["alternate", "blurred"]);
    assert_eq!(q.nsfw, Some(Tri::Exclude));
    assert_eq!(q.humor, Some(Tri::Any));
    assert_eq!(q.epilepsy, Some(Tri::Only));
    assert_eq!(q.oneoftag, vec!["humor", "nsfw"]);
}

#[test]
fn an_empty_oneoftag_means_no_tag_filter_and_is_not_sent() {
    // `filtersToQuery` emits `oneoftag: ''` when `untagged` is on. Sending that verbatim
    // would ask SteamGridDB to match a tag whose name is the empty string.
    let q = AssetQuery::from_params(AssetKind::Grid, &params(&[("oneoftag", "")])).unwrap();
    assert!(q.oneoftag.is_empty());
    assert!(
        !q.to_pairs().iter().any(|(k, _)| *k == "oneoftag"),
        "an empty oneoftag must be omitted from the URL entirely",
    );

    // Control: a non-empty one really does get sent, so the assertion above is about the
    // empty case and not about oneoftag being broken outright.
    let q = AssetQuery::from_params(AssetKind::Grid, &params(&[("oneoftag", "humor")])).unwrap();
    assert!(q.to_pairs().contains(&("oneoftag", "humor".to_owned())));
}

#[test]
fn a_value_outside_the_closed_set_is_refused_locally_and_names_itself() {
    assert_eq!(
        AssetQuery::from_params(AssetKind::Grid, &params(&[("dimensions", "1x1")])),
        Err(QueryError::UnknownDimension {
            value: "1x1".to_owned()
        }),
    );
    assert_eq!(
        AssetQuery::from_params(AssetKind::Grid, &params(&[("styles", "sparkly")])),
        Err(QueryError::UnknownStyle {
            value: "sparkly".to_owned(),
            endpoint: "grids",
        }),
    );
    assert_eq!(
        AssetQuery::from_params(AssetKind::Icon, &params(&[("mimes", "image/gif")])),
        Err(QueryError::UnknownMime {
            value: "image/gif".to_owned(),
            endpoint: "icons",
        }),
    );
    assert_eq!(
        AssetQuery::from_params(AssetKind::Grid, &params(&[("types", "moving")])),
        Err(QueryError::UnknownType {
            value: "moving".to_owned()
        }),
    );
    // A boolean is the obvious wrong guess here: the API takes any/true/false, not a bool.
    assert_eq!(
        AssetQuery::from_params(AssetKind::Grid, &params(&[("nsfw", "yes")])),
        Err(QueryError::BadTri {
            param: "nsfw",
            value: "yes".to_owned()
        }),
    );
}

#[test]
fn a_style_valid_for_one_endpoint_is_refused_for_another() {
    // `white_logo` is a grid style; the logo endpoint has its own vocabulary. Accepting it
    // everywhere would send a 400 that reads as a service failure.
    assert!(AssetQuery::from_params(AssetKind::Grid, &params(&[("styles", "white_logo")])).is_ok());
    assert!(
        AssetQuery::from_params(AssetKind::Logo, &params(&[("styles", "white_logo")])).is_err()
    );
    // And the reverse, so this is about vocabularies rather than about one being stricter.
    assert!(AssetQuery::from_params(AssetKind::Logo, &params(&[("styles", "black")])).is_ok());
    assert!(AssetQuery::from_params(AssetKind::Grid, &params(&[("styles", "black")])).is_err());
}

#[test]
fn tri_parses_exactly_the_three_values_the_api_takes() {
    assert_eq!(Tri::parse("any"), Some(Tri::Any));
    assert_eq!(Tri::parse("true"), Some(Tri::Only));
    assert_eq!(Tri::parse("false"), Some(Tri::Exclude));
    assert_eq!(Tri::parse("TRUE"), None, "the API is case-sensitive here");
    assert_eq!(Tri::parse(""), None);
}

#[test]
fn the_filter_vocabulary_matches_the_shared_fixture() {
    // The anti-drift guard. These lists exist in both Rust and TypeScript: the UI offers
    // values from the TS tables and `from_params` validates against the Rust ones, so a
    // value in one and not the other becomes a filter that silently returns nothing.
    // Same pattern, and the same reason, as the logo fixture.
    #[derive(serde::Deserialize)]
    struct Vocabulary {
        dimensions: std::collections::BTreeMap<String, DimensionSet>,
        styles: std::collections::BTreeMap<String, Vec<String>>,
        mimes: std::collections::BTreeMap<String, Vec<String>>,
        types: Vec<String>,
        tri: Vec<String>,
        #[serde(rename = "pageLimit")]
        page_limit: u32,
    }
    #[derive(serde::Deserialize)]
    struct DimensionSet {
        all: Vec<String>,
    }

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/shared/fixtures/filter-vocabulary.json"
    );
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("shared filter fixture missing at {path}: {e}"));
    let vocab: Vocabulary = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("shared filter fixture malformed: {e}"));

    // Premise: the fixture really was loaded and is not an empty object that would make
    // every assertion below vacuous.
    assert_eq!(vocab.dimensions.len(), 5, "one entry per asset type");

    // Every dimension named anywhere in the fixture must be parseable by Rust.
    for (asset, set) in &vocab.dimensions {
        for value in &set.all {
            assert!(
                Dimensions::parse(value).is_some(),
                "{asset}: TypeScript offers dimension {value:?} that Rust would reject",
            );
        }
    }
    // ...and the reverse, so Rust cannot carry a value the UI can never produce.
    for d in Dimensions::ALL {
        assert!(
            vocab
                .dimensions
                .values()
                .any(|s| s.all.iter().any(|v| v == d.as_str())),
            "Rust knows dimension {} that the shared fixture does not list",
            d.as_str(),
        );
    }

    let kind_of = |asset: &str| match asset {
        "grid_p" | "grid_l" => AssetKind::Grid,
        "hero" => AssetKind::Hero,
        "logo" => AssetKind::Logo,
        "icon" => AssetKind::Icon,
        other => panic!("unknown asset type {other:?} in the fixture"),
    };

    for (asset, styles) in &vocab.styles {
        let kind = kind_of(asset);
        for value in styles {
            assert!(
                kind.styles().contains(&value.as_str()),
                "{asset}: TypeScript offers style {value:?} that Rust would reject",
            );
        }
    }
    for (asset, mimes) in &vocab.mimes {
        let kind = kind_of(asset);
        for value in mimes {
            assert!(
                kind.mimes().contains(&value.as_str()),
                "{asset}: TypeScript offers MIME {value:?} that Rust would reject",
            );
        }
    }

    assert_eq!(vocab.types, ASSET_TYPE_FILTERS);
    assert_eq!(
        vocab.tri,
        vec!["any".to_owned(), "true".to_owned(), "false".to_owned()],
    );
    assert_eq!(vocab.page_limit, PAGE_LIMIT);
}

#[test]
fn the_two_grid_slots_hit_the_same_endpoint_with_different_dimensions() {
    // The mapping most likely to be got wrong, and the one whose failure looks like
    // "the art applied but it is the wrong shape".
    let (capsule_kind, capsule) = AssetQuery::for_asset_type(AssetType::Capsule).unwrap();
    let (header_kind, header) = AssetQuery::for_asset_type(AssetType::Header).unwrap();

    assert_eq!(capsule_kind, AssetKind::Grid);
    assert_eq!(header_kind, AssetKind::Grid);
    assert_eq!(capsule_kind.path(), "grids");

    assert!(capsule.dimensions.contains(&Dimensions::D600x900));
    assert!(header.dimensions.contains(&Dimensions::D460x215));
    assert!(
        !capsule
            .dimensions
            .iter()
            .any(|d| header.dimensions.contains(d)),
        "portrait and wide dimension sets must not overlap"
    );
}

#[test]
fn every_editable_slot_maps_somewhere_except_heroblur() {
    for t in [
        AssetType::Capsule,
        AssetType::Header,
        AssetType::Hero,
        AssetType::Logo,
        AssetType::Icon,
    ] {
        assert!(
            AssetKind::for_asset_type(t).is_some(),
            "{t} must map to an endpoint"
        );
    }
    assert!(
        AssetKind::for_asset_type(AssetType::HeroBlur).is_none(),
        "HeroBlur is generated by Steam; nobody uploads one"
    );
}

#[test]
fn logos_and_icons_carry_no_dimension_filter() {
    // Icons matter most here: the endpoint 400s on *every* dimension value, so sending one
    // would break the whole tab rather than just narrowing it.
    for t in [AssetType::Logo, AssetType::Icon] {
        let (_, q) = AssetQuery::for_asset_type(t).unwrap();
        assert!(q.dimensions.is_empty(), "{t} must send no dimensions");
        assert!(
            !q.to_pairs().iter().any(|(k, _)| *k == "dimensions"),
            "an empty dimensions list must be omitted, not sent as `dimensions=`"
        );
    }
}

#[test]
fn tri_renders_the_api_words_not_booleans() {
    assert_eq!(Tri::Any.as_str(), "any");
    assert_eq!(Tri::Only.as_str(), "true");
    assert_eq!(Tri::Exclude.as_str(), "false");
    assert_eq!(Tri::default(), Tri::Exclude);
}

#[test]
fn pairs_render_as_comma_separated_and_omit_empties() {
    let q = AssetQuery {
        dimensions: vec![Dimensions::D600x900, Dimensions::D342x482],
        nsfw: Some(Tri::Any),
        oneoftag: vec!["humor".into()],
        page: Some(2),
        limit: Some(50),
        ..Default::default()
    };
    let pairs = q.to_pairs();
    let get = |k: &str| {
        pairs
            .iter()
            .find(|(pk, _)| *pk == k)
            .map(|(_, v)| v.clone())
    };

    assert_eq!(get("dimensions"), Some("600x900,342x482".into()));
    assert_eq!(get("nsfw"), Some("any".into()));
    assert_eq!(get("oneoftag"), Some("humor".into()));
    assert_eq!(get("page"), Some("2".into()));
    assert_eq!(get("limit"), Some("50".into()));
    assert_eq!(get("styles"), None);
    assert_eq!(get("humor"), None, "an unset filter must be omitted");
}

#[test]
fn an_empty_query_sends_nothing_at_all() {
    assert!(AssetQuery::default().to_pairs().is_empty());
}

#[test]
fn every_dimension_knows_which_endpoint_it_belongs_to() {
    // Measured: grids accept 600x900/342x482/660x930/460x215/920x430/512x512/1024x1024;
    // heroes accept only their own three and 400 on any grid value.
    for d in Dimensions::PORTRAIT
        .iter()
        .chain(Dimensions::WIDE)
        .chain(Dimensions::GRID_SQUARE)
    {
        assert_eq!(d.endpoint(), AssetKind::Grid, "{}", d.as_str());
    }
    for d in Dimensions::HERO {
        assert_eq!(d.endpoint(), AssetKind::Hero, "{}", d.as_str());
    }
}

#[test]
fn a_dimension_from_the_wrong_endpoint_is_refused_locally() {
    // `heroes?dimensions=600x900` is an HTTP 400. Catching it here means the message names
    // the actual mistake instead of blaming SteamGridDB.
    let q = AssetQuery {
        dimensions: vec![Dimensions::D600x900],
        ..Default::default()
    };
    let err = q.validate_for(AssetKind::Hero).unwrap_err();
    assert!(err.contains("600x900"), "{err}");
    assert!(err.contains("heroes"), "{err}");

    assert!(q.validate_for(AssetKind::Grid).is_ok(), "the control");
}

#[test]
fn any_dimension_on_icons_or_logos_is_refused() {
    // Both endpoints 400 on every value, including plausible ones like 512x512.
    let q = AssetQuery {
        dimensions: vec![Dimensions::D512x512],
        ..Default::default()
    };
    for kind in [AssetKind::Icon, AssetKind::Logo] {
        let err = q.validate_for(kind).unwrap_err();
        assert!(err.contains("rejects `dimensions` entirely"), "{err}");
    }
}

#[test]
fn the_queries_we_build_ourselves_all_validate() {
    // The guard must never fire on our own defaults — that would break every tab.
    for t in [
        AssetType::Capsule,
        AssetType::Header,
        AssetType::Hero,
        AssetType::Logo,
        AssetType::Icon,
    ] {
        let (kind, q) = AssetQuery::for_asset_type(t).unwrap();
        assert!(
            q.validate_for(kind).is_ok(),
            "{t} produced an invalid query"
        );
    }
}
