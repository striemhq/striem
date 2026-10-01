//! The detection service's rule engine.
//!
//! This module follows rsigma-runtime's `RuntimeEngine` and `LogProcessor`. It
//! is not those types, because `RuntimeEngine` fixes its logsource extractor to
//! `FieldLogSourceExtractor`, which reads the logsource from the event body.
//! StrIEM keeps the logsource in the event metadata. Thus this engine fixes the
//! extractor to [`VectorLogSourceExtractor`] and evaluates [`LogsourceEvent`]s.
//! The rsigma-eval engines under it are generic over the extractor, so the
//! evaluation itself is rsigma's.
//!
//! - [`RuntimeEngine`] owns one compiled rule set. It is a detection-only
//!   [`Engine`], or a [`CorrelationEngine`] when the rules have correlations.
//! - [`Processor`] holds the current `RuntimeEngine` for the worker and the
//!   rule-admin API. It swaps in a new engine on reload. A batch that is in
//!   progress finishes on the engine that it started with.
//!
//! The service does not use rsigma-runtime's schema routing, dynamic pipeline
//! sources, or bloom and Aho-Corasick pre-filters, so this engine leaves them
//! out.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use arc_swap::ArcSwap;
use log::warn;
use rsigma_eval::{
    CorrelationConfig, CorrelationEngine, CorrelationSnapshot, Engine, Pipeline, ProcessResult,
};
use rsigma_parser::SigmaCollection;

use crate::event::LogSourceEvent;
use crate::logsource::VectorLogSourceExtractor;

/// Counts about the loaded rule set.
#[derive(Debug, Clone, Copy)]
pub struct EngineStats {
    pub detection_rules: usize,
    pub correlation_rules: usize,
    pub state_entries: usize,
}

enum EngineVariant {
    DetectionOnly(Box<Engine<VectorLogSourceExtractor>>),
    WithCorrelations(Box<CorrelationEngine<VectorLogSourceExtractor>>),
}

/// One compiled rule set, with the configuration to compile it again.
pub struct RuntimeEngine {
    engine: EngineVariant,
    pipelines: Vec<Pipeline>,
    rules_path: PathBuf,
    corr_config: CorrelationConfig,
    include_event: bool,
    /// The conflict-pruning extractor. `None` turns pruning off. Each
    /// `load_rules()` gives it to the new inner engine.
    logsource_extractor: Option<VectorLogSourceExtractor>,
}

impl RuntimeEngine {
    /// Makes an engine with no rules. Call [`load_rules`](Self::load_rules) to
    /// compile the rules at `rules_path`.
    pub fn new(
        rules_path: PathBuf,
        pipelines: Vec<Pipeline>,
        corr_config: CorrelationConfig,
        include_event: bool,
    ) -> Self {
        RuntimeEngine {
            engine: EngineVariant::DetectionOnly(Box::new(new_engine(None))),
            pipelines,
            rules_path,
            corr_config,
            include_event,
            logsource_extractor: None,
        }
    }

    /// Sets the logsource extractor. The next `load_rules()` uses it. Set it
    /// before the first load: without it, the engine does not prune.
    pub fn set_logsource_extractor(&mut self, extractor: Option<VectorLogSourceExtractor>) {
        self.logsource_extractor = extractor;
    }

    /// Gives the logsource extractor, if there is one.
    pub fn logsource_extractor(&self) -> Option<VectorLogSourceExtractor> {
        self.logsource_extractor.clone()
    }

    /// Loads (or loads again) the rules from the rules path, and compiles them.
    ///
    /// On a reload, the correlation state goes out of the old engine and into
    /// the new one. Thus open windows survive a rule change. The new engine
    /// drops the state of a correlation that is gone. If the load fails, the
    /// old engine stays.
    pub fn load_rules(&mut self) -> Result<EngineStats, String> {
        let previous_state = self.export_state();
        let collection = load_collection(&self.rules_path)?;

        if collection.correlations.is_empty() {
            let mut engine = new_engine(self.logsource_extractor.clone());
            engine.set_include_event(self.include_event);
            for p in &self.pipelines {
                engine.add_pipeline(p.clone());
            }
            engine
                .add_collection(&collection)
                .map_err(|e| format!("Error compiling rules: {e}"))?;

            self.engine = EngineVariant::DetectionOnly(Box::new(engine));
        } else {
            let mut engine = new_correlation_engine(
                self.corr_config.clone(),
                self.logsource_extractor.clone(),
            );
            engine.set_include_event(self.include_event);
            for p in &self.pipelines {
                engine.add_pipeline(p.clone());
            }
            engine
                .add_collection(&collection)
                .map_err(|e| format!("Error compiling rules: {e}"))?;
            if let Some(snapshot) = previous_state
                && !engine.import_state(snapshot)
            {
                warn!("incompatible correlation snapshot version during reload, starting fresh");
            }

            self.engine = EngineVariant::WithCorrelations(Box::new(engine));
        }

        Ok(self.stats())
    }

    /// Evaluates a batch of events: detection in parallel, then correlation in
    /// order. Gives one result per event.
    pub fn process_batch(&mut self, events: &[&LogSourceEvent<'_>]) -> Vec<ProcessResult> {
        match &mut self.engine {
            EngineVariant::DetectionOnly(engine) => engine.evaluate_batch(events),
            EngineVariant::WithCorrelations(engine) => engine.process_batch(events),
        }
    }

    /// Gives counts about the current rule set.
    pub fn stats(&self) -> EngineStats {
        match &self.engine {
            EngineVariant::DetectionOnly(engine) => EngineStats {
                detection_rules: engine.rule_count(),
                correlation_rules: 0,
                state_entries: 0,
            },
            EngineVariant::WithCorrelations(engine) => EngineStats {
                detection_rules: engine.detection_rule_count(),
                correlation_rules: engine.correlation_rule_count(),
                state_entries: engine.state_count(),
            },
        }
    }

    /// Gives the path that the rules load from.
    pub fn rules_path(&self) -> &Path {
        &self.rules_path
    }

    /// Gives the correlation state. Gives `None` for a detection-only engine.
    pub fn export_state(&self) -> Option<CorrelationSnapshot> {
        match &self.engine {
            EngineVariant::DetectionOnly(_) => None,
            EngineVariant::WithCorrelations(engine) => Some(engine.export_state()),
        }
    }

    /// Makes an empty engine with the same configuration. A reload compiles the
    /// rules into it, then swaps it in.
    fn empty_copy(&self) -> Self {
        let mut engine = RuntimeEngine::new(
            self.rules_path.clone(),
            self.pipelines.clone(),
            self.corr_config.clone(),
            self.include_event,
        );
        engine.set_logsource_extractor(self.logsource_extractor.clone());
        engine
    }
}

/// Makes a detection engine. rsigma-eval can only fix a non-default extractor
/// type through `with_logsource_extractor`. Thus with no extractor, give it one,
/// then remove it.
fn new_engine(extractor: Option<VectorLogSourceExtractor>) -> Engine<VectorLogSourceExtractor> {
    let mut engine = Engine::with_logsource_extractor(VectorLogSourceExtractor::new());
    engine.set_logsource_extractor(extractor);
    engine
}

/// Makes a correlation engine in the same way as [`new_engine`].
fn new_correlation_engine(
    config: CorrelationConfig,
    extractor: Option<VectorLogSourceExtractor>,
) -> CorrelationEngine<VectorLogSourceExtractor> {
    let mut engine =
        CorrelationEngine::with_logsource_extractor(config, VectorLogSourceExtractor::new());
    engine.set_logsource_extractor(extractor);
    engine
}

/// Parses the rules at `path`, a directory or one file. A rule that does not
/// parse is logged and skipped. It does not stop the load.
fn load_collection(path: &Path) -> Result<SigmaCollection, String> {
    let collection = if path.is_dir() {
        rsigma_parser::parse_sigma_directory(path)
            .map_err(|e| format!("Error loading rules from {}: {e}", path.display()))?
    } else {
        rsigma_parser::parse_sigma_file(path)
            .map_err(|e| format!("Error loading rule {}: {e}", path.display()))?
    };

    if !collection.errors.is_empty() {
        warn!("{} parse error(s) while loading rules", collection.errors.len());
        for err in collection.errors.iter().take(3) {
            warn!("rule parse error: {err}");
        }
    }

    Ok(collection)
}

/// Holds the current [`RuntimeEngine`] and swaps it on a reload.
///
/// The engine is behind `ArcSwap<RwLock<_>>`, as in rsigma-runtime's
/// `LogProcessor`. A batch takes the engine's write lock (correlation state
/// changes on each batch). A reload compiles the new engine with no lock held,
/// then swaps the pointer. A batch that is in progress keeps its own `Arc` to
/// the old engine and finishes on it.
pub struct Processor {
    engine: ArcSwap<RwLock<RuntimeEngine>>,
}

impl Processor {
    pub fn new(engine: RuntimeEngine) -> Self {
        Processor {
            engine: ArcSwap::from_pointee(RwLock::new(engine)),
        }
    }

    /// Evaluates a batch on the current engine.
    pub fn process_batch(&self, events: &[&LogSourceEvent<'_>]) -> Vec<ProcessResult> {
        let snapshot = self.engine.load();
        let mut engine = snapshot.write().unwrap_or_else(PoisonError::into_inner);
        engine.process_batch(events)
    }

    /// Compiles the rules again into a new engine, then swaps it in. The
    /// extractor and the correlation state carry over. If the load fails, the
    /// current engine stays.
    pub fn reload_rules(&self) -> Result<EngineStats, String> {
        let (mut next, state) = {
            let snapshot = self.engine.load();
            let current = snapshot.read().unwrap_or_else(PoisonError::into_inner);
            (current.empty_copy(), current.export_state())
        };

        let mut stats = next.load_rules()?;
        if let Some(state) = state {
            if let EngineVariant::WithCorrelations(engine) = &mut next.engine
                && !engine.import_state(state)
            {
                warn!("incompatible correlation snapshot version during reload, starting fresh");
            }
            stats = next.stats();
        }

        self.engine.store(Arc::new(RwLock::new(next)));
        Ok(stats)
    }

    /// Gives the path that the rules load from.
    pub fn rules_path(&self) -> PathBuf {
        let snapshot = self.engine.load();
        let engine = snapshot.read().unwrap_or_else(PoisonError::into_inner);
        engine.rules_path().to_path_buf()
    }

    /// Gives counts about the current rule set.
    pub fn stats(&self) -> EngineStats {
        let snapshot = self.engine.load();
        let engine = snapshot.read().unwrap_or_else(PoisonError::into_inner);
        engine.stats()
    }
}
