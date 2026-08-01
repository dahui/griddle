//! Browsing SteamGridDB, and deciding which of its games an appid corresponds to.

use super::{Res, parse_asset_type};
use crate::error::{Kind, UiError};
use crate::state::AppState;
use griddle_core::appid::AppId;
use griddle_core::sgdb::{self, AssetQuery, Target};
use griddle_core::steam::shortcuts::Shortcuts;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub assets: Vec<griddle_core::sgdb::Asset>,
    pub page: u32,
    pub total: u32,
    pub has_more: bool,
}

/// One page of artwork for a game.
///
/// `filters` is the output of `filtersToQuery()` in `packages/shared/src/filters.ts`. It is
/// optional so the tab's defaults still apply before the user has touched anything.
#[tauri::command]
pub async fn search_assets(
    state: State<'_, AppState>,
    app_id: u32,
    asset_type: String,
    page: u32,
    filters: Option<sgdb::FilterParams>,
) -> Res<SearchResult> {
    let asset = parse_asset_type(&asset_type)?;
    let Some((kind, base)) = AssetQuery::for_asset_type(asset) else {
        return Err(UiError::new(
            Kind::Unexpected,
            format!("{asset} has no SteamGridDB source"),
        ));
    };

    let mut query = match &filters {
        Some(params) => AssetQuery::from_params(kind, params).map_err(|e| {
            UiError::new(Kind::Unexpected, e.to_string())
                .with_action("Reset the filters for this tab.")
        })?,
        None => base.clone(),
    };

    // Restore the tab's dimensions when the filter set carries none.
    //
    // Both grid slots use the *same* endpoint and are told apart only by `dimensions`. Querying
    // `grids` with none for the Header tab fills it with portrait art, which then gets written
    // into the wide slot — it applies, and it looks wrong, which is worse than failing.
    if query.dimensions.is_empty() {
        query.dimensions = base.dimensions;
    }

    // Override, then appid, then a search by name — see `resolve_game`. Resolved before the
    // client is locked, because resolving needs that same lock.
    //
    // Falling back to `Target::Steam` when nothing resolves is deliberate: the API's own 404 is
    // a clearer, more specific error than anything invented here, and it carries the "search by
    // name instead" action.
    let target = match resolve_game(&state, app_id).await? {
        Some(game) => Target::Sgdb(game.id),
        None => Target::Steam(AppId::new(app_id)),
    };

    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;

    let query = query.page(page).limit(sgdb::PAGE_LIMIT);
    let result = client.assets(kind, target, &query).await?;

    Ok(SearchResult {
        page: result.page,
        total: result.total,
        has_more: result.has_more(),
        assets: result.assets,
    })
}

/// How an appid was matched to a SteamGridDB game.
///
/// Internal: it distinguishes a manual override from an automatic match, which is what lets
/// [`current_game_match`] re-resolve the name of an override stored before names were kept.
/// It is deliberately **not** sent to the UI — the label is just the game's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedBy {
    /// The user picked this game themselves.
    Override,
    /// SteamGridDB knows this Steam appid. Exact.
    AppId,
    /// SteamGridDB does not know the appid, so the game's name was searched instead.
    Name,
}

/// A SteamGridDB game, for the "wrong game?" picker.
#[derive(Debug, Clone, Serialize)]
pub struct GameMatch {
    /// SteamGridDB's own id — **not** a Steam appid.
    pub id: u64,
    pub name: String,
    pub verified: bool,
    pub types: Vec<String>,
    /// Not serialised: the UI shows the game's name, not how it was found.
    #[serde(skip)]
    pub matched_by: MatchedBy,
}

impl GameMatch {
    fn from_game(g: sgdb::Game, matched_by: MatchedBy) -> Self {
        GameMatch {
            id: g.id,
            name: g.name,
            verified: g.verified,
            types: g.types,
            matched_by,
        }
    }
}

/// The name to search SteamGridDB with when the appid is not known there.
///
/// `appinfo.vdf` covers Steam apps; `shortcuts.vdf` covers non-Steam ones, whose appid is a
/// random high-bit number SteamGridDB has never seen and never will — so for those the name is
/// the *only* way to match anything at all.
fn searchable_name(ctx: &crate::state::SteamContext, app_id: u32) -> Option<String> {
    let app = AppId::new(app_id);
    if let Some(name) = ctx.app_types.as_ref().and_then(|t| t.name(app)) {
        return Some(name.to_owned());
    }
    let shortcuts = Shortcuts::load_or_empty(ctx.install.shortcuts_vdf(ctx.account.id)).ok()?;
    shortcuts
        .iter()
        .find(|s| s.app_id() == Some(app))
        .and_then(|s| s.app_name())
        .map(str::to_owned)
}

/// Which SteamGridDB game an appid pulls artwork from.
///
/// The ladder, and why the last rung exists:
///
/// 1. **A manual override** — the user's choice always wins.
/// 2. **The Steam appid** — exact, and right for most games.
/// 3. **A name search** — because plenty of appids are not on SteamGridDB at all. Measured:
///    `3837340` (FINAL FANTASY VII) 404s by appid but its name finds the game immediately, and
///    every non-Steam shortcut 404s by construction since its appid is a random number Steam
///    generated locally. `[VERIFIED-BOX 2026-07-30]` Without this rung those games showed an
///    error instead of artwork.
///
/// Cached per session in [`AppState::game_matches`], including a `None`, so a game with no match
/// does not re-search on every page of a scroll.
async fn resolve_game(state: &State<'_, AppState>, app_id: u32) -> Res<Option<GameMatch>> {
    // The user's own choice, and the only mapping that is persisted.
    if let Some(over) = state.settings.lock().await.game_overrides.get(&app_id) {
        return Ok(Some(GameMatch {
            id: over.id,
            name: over
                .name
                .clone()
                .unwrap_or_else(|| format!("SteamGridDB game #{}", over.id)),
            verified: false,
            types: Vec::new(),
            matched_by: MatchedBy::Override,
        }));
    }

    if let Some(cached) = state.game_matches.lock().await.get(&app_id) {
        return Ok(cached.clone());
    }

    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;

    let mut resolved = client
        .game_by_steam_appid(AppId::new(app_id))
        .await?
        .map(|g| GameMatch::from_game(g, MatchedBy::AppId));

    if resolved.is_none()
        && let Ok(ctx) = state.steam()
        && let Some(name) = searchable_name(ctx, app_id)
    {
        // First hit only. SteamGridDB's autocomplete is already ranked, and offering the user a
        // list here would be the "Wrong game?" picker — which they can still open to change it.
        resolved = client
            .search(&name)
            .await?
            .into_iter()
            .next()
            .map(|g| GameMatch::from_game(g, MatchedBy::Name));
        match &resolved {
            Some(g) => tracing::info!(app_id, name, matched = g.name, "matched by name"),
            None => tracing::info!(app_id, name, "no SteamGridDB match by appid or name"),
        }
    }

    let _ = state
        .game_matches
        .lock()
        .await
        .insert(app_id, resolved.clone());
    Ok(resolved)
}

/// Search SteamGridDB by name, for when the automatic Steam-appid match is wrong or absent.
///
/// The request is made entirely in Rust; only results cross the boundary. The API key stays in
/// `sgdb::client` — it cannot be serialised, so this is enforced by the compiler rather than by
/// remembering.
#[tauri::command]
pub async fn search_games(state: State<'_, AppState>, term: String) -> Res<Vec<GameMatch>> {
    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;
    // An empty term short-circuits inside the client without a request.
    Ok(client
        .search(&term)
        .await?
        .into_iter()
        // These are candidates the user is choosing between, so they are all "by name" until
        // one is picked and becomes an override.
        .map(|g| GameMatch::from_game(g, MatchedBy::Name))
        .collect())
}

/// Which SteamGridDB game this appid currently resolves to: the manual override if one is set,
/// otherwise the automatic match. `None` means SteamGridDB has no entry for it.
#[tauri::command]
pub async fn current_game_match(state: State<'_, AppState>, app_id: u32) -> Res<Option<GameMatch>> {
    let resolved = resolve_game(&state, app_id).await?;

    // An override stored before the name was kept alongside it shows as `SteamGridDB game
    // #17830`. `/games/id/{id}` is probed and works, so resolve it properly.
    if let Some(game) = &resolved
        && game.matched_by == MatchedBy::Override
        && game.name.starts_with("SteamGridDB game #")
    {
        let guard = state.sgdb.lock().await;
        let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;
        if let Some(found) = client.game_by_id(game.id).await? {
            return Ok(Some(GameMatch::from_game(found, MatchedBy::Override)));
        }
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use griddle_core::grid::names::AssetType;

    /// The dimension-restoring rule from `search_assets`, extracted so it can be tested without
    /// a Tauri `State` or a network call. Kept next to its caller so the two cannot drift.
    fn effective_dimensions(
        asset: AssetType,
        filters: Option<&sgdb::FilterParams>,
    ) -> Vec<griddle_core::sgdb::Dimensions> {
        let (kind, base) = AssetQuery::for_asset_type(asset).unwrap();
        let mut query = match filters {
            Some(p) => AssetQuery::from_params(kind, p).unwrap(),
            None => base.clone(),
        };
        if query.dimensions.is_empty() {
            query.dimensions = base.dimensions;
        }
        query.dimensions
    }

    #[test]
    fn a_filter_set_with_no_dimensions_keeps_the_tabs_own_dimensions() {
        use griddle_core::sgdb::Dimensions;

        // The most dangerous line in the filter path. Capsule and Header are the *same*
        // endpoint, told apart only by `dimensions`. Letting an empty filter set through would
        // fill the Header tab with portrait art, which then applies to the wide slot — it works,
        // and it looks wrong, which is worse than failing.
        let empty = sgdb::FilterParams::default();

        // Premise: the filter set really does carry no dimensions, so this exercises the guard.
        assert!(empty.dimensions.is_none());

        assert_eq!(
            effective_dimensions(AssetType::Header, Some(&empty)),
            Dimensions::WIDE,
        );
        assert_eq!(
            effective_dimensions(AssetType::Capsule, Some(&empty)),
            Dimensions::PORTRAIT,
        );

        // The control: an explicit choice is honoured rather than overwritten, or the guard
        // would silently make the size filter do nothing at all.
        let chosen = sgdb::FilterParams {
            dimensions: Some("920x430".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            effective_dimensions(AssetType::Header, Some(&chosen)),
            vec![Dimensions::D920x430],
        );
    }
}
