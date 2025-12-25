# Unified Model and Provider Selection with Caching

## Objective

Implement a unified model and provider selection interface that allows users to select both model and provider simultaneously. The system should fetch and cache models from all logged-in providers at once, display provider information alongside models, automatically set the provider when a model is selected, and maintain high performance through intelligent caching that works seamlessly in both zsh and REPL environments.

## Implementation Plan

- [x] 1. **Add provider_id field to Model domain structure**
  - Modify the Model struct in `crates/forge_domain/src/model.rs:7` to include `pub provider_id: ProviderId` field
  - This addresses the TODO comment at `crates/forge_domain/src/model.rs:12` about adding provider information to the model
  - Update all Model constructors and tests to include provider_id
  - Rationale: Models need to carry provider context through the entire system for unified selection and proper provider switching

- [x] 2. **Update DTO to Domain conversions to preserve provider context**
  - Modify the From implementation in `crates/forge_app/src/dto/openai/model.rs:91-114` to accept and set provider_id during conversion
  - Update the From implementation in `crates/forge_app/src/dto/anthropic/response.rs:21-33` similarly
  - Change the conversion functions to take provider_id as a parameter or use a builder pattern
  - Rationale: Provider context is currently lost during DTO conversion, this ensures it flows through to the domain model

- [x] 3. **Enhance provider service to attach provider_id when fetching models**
  - Modify the models method in `crates/forge_services/src/provider/service.rs:90-112` to set provider_id on each returned Model
  - Update the client's models method in `crates/forge_services/src/provider/client.rs` to accept and propagate provider_id
  - Update the inner_models method in `crates/forge_services/src/provider/openai.rs:123-156` to set provider_id on fetched models
  - Rationale: Ensures provider context is attached at the point where models are fetched from provider APIs

- [x] 4. **Create new API method to fetch models from all configured providers**
  - Add get_all_models method in `crates/forge_app/src/app.rs` that fetches models from all providers returned by get_all_providers
  - Use parallel async fetching with tokio join_all or FuturesUnordered to fetch from all providers concurrently
  - Filter to only include LLM providers using ProviderType filter similar to `crates/forge_main/src/ui.rs:2114`
  - Handle errors gracefully per provider - if one provider fails, still return models from others
  - Return a flattened Vec of all models with provider_id populated
  - Rationale: Current get_models only fetches from default provider, we need models from all logged-in providers for unified selection

- [x] 5. **Add API endpoint for get_all_models in forge_api layer**
  - Create get_all_models method in `crates/forge_api/src/api.rs:24` trait
  - Implement it in `crates/forge_api/src/forge_api.rs` to call the app layer's get_all_models
  - Ensure proper error handling and result mapping
  - Rationale: Maintains clean architecture by exposing the new functionality through the API layer

- [x] 6. **Enhance cached_models to support multi-provider aggregation**
  - Consider renaming or adding a new cache field in `crates/forge_services/src/provider/service.rs:20` for aggregated models
  - Add a new cache entry that stores all models across providers with a special key or separate field
  - Implement cache invalidation logic - when any provider's credentials change, clear the aggregated cache
  - Add timestamp to cache entries to implement TTL (time-to-live) for auto-expiration
  - **IMPLEMENTED**: Added file-based persistent caching to `~/forge/cache/models.json` for zsh compatibility
  - Rationale: Current cache is per-provider, we need efficient caching for the aggregated multi-provider model list

- [x] 7. **Implement cache TTL and refresh strategy**
  - Add a timestamp field to cached entries in the cached_models HashMap
  - Implement a configurable TTL (e.g., 1 hour) that can be set via environment or config
  - Add a check before returning cached models to verify they haven't exceeded TTL
  - If TTL expired, trigger background refresh while returning stale data, or block and fetch fresh data based on configuration
  - Rationale: Prevents showing outdated model lists when providers add new models, balancing freshness with performance

- [x] 8. **Update CliModel display to show provider information**
  - Modify the Display implementation in `crates/forge_main/src/model.rs:18-51` to include provider name or identifier
  - Format could be: "gpt-4 [ 128k 🛠️ ] [OpenAI]" or use provider as a prefix: "[OpenAI] gpt-4 [ 128k 🛠️ ]"
  - Retrieve provider name from the model's provider_id field
  - Consider adding a helper method to format provider display name
  - Rationale: Users need to see which provider each model belongs to for informed selection

- [x] 9. **Create new get_all_models method in UI layer**
  - Add get_all_models method in `crates/forge_main/src/ui.rs` similar to the existing get_models at line 122-127
  - Call the new API endpoint api.get_all_models
  - Handle the spinner for loading indication
  - Cache the results locally in the UI for quick subsequent access during the same session
  - Rationale: Provides UI-level access to multi-provider model list with proper loading UX

- [x] 10. **Update select_model to use multi-provider model list**
  - Modify the select_model method in `crates/forge_main/src/ui.rs:1741-1783` to call get_all_models instead of get_models
  - Update sorting logic to sort by provider first, then by model name, or add a filter option
  - Consider adding a provider group header in the selection UI to visually separate models by provider
  - Ensure starting_cursor logic still works correctly with the expanded model list
  - Rationale: Enables unified model selection showing all available models across providers

- [x] 11. **Implement automatic provider switching on model selection**
  - After user selects a model in on_model_selection method at `crates/forge_main/src/ui.rs:2151-2170`, extract the provider_id from the selected model
  - Check if the selected model's provider differs from the current default provider
  - If different, call api.set_default_provider with the model's provider_id before calling api.set_default_model
  - Update UI state to reflect both the new model and provider
  - Display a message like "Switched to model: {model} on provider: {provider}"
  - Rationale: Seamless user experience - selecting a model from a different provider automatically switches to that provider

- [x] 12. **Update model list command to show provider column**
  - Modify on_show_models method in `crates/forge_main/src/ui.rs:1028-1076` to fetch all models from all providers
  - Add a provider column to the Info output using add_key_value for the provider name
  - For porcelain format, add provider as a column in the table
  - Sort output by provider first, then by model name for better readability
  - Rationale: Provides clear visibility of which models belong to which provider in list commands

- [x] 13. **Optimize cache hydration for multi-provider scenario**
  - Update hydrate_caches method in `crates/forge_main/src/ui.rs:348-360` to spawn a task for get_all_models instead of get_models
  - Consider adding a priority system - fetch models from default provider first, then others
  - Add error logging if multi-provider fetch fails, but don't block app startup
  - Rationale: Pre-warms the aggregated cache on startup for instant model selection

- [x] 14. **Add configuration option for model list caching behavior**
  - Add a new config field in forge.yaml schema for model_cache_ttl_seconds
  - Add a config field for model_cache_strategy with options like "aggressive" (cache indefinitely), "moderate" (1 hour TTL), "fresh" (always fetch)
  - Update the config loading in `crates/forge_app/src/app.rs` to read and apply these settings
  - Use these settings in the provider service caching logic
  - Rationale: Gives users control over cache behavior based on their needs and network conditions

- [ ] 15. **Implement ForgeSelect enhancement for provider grouping**
  - Consider enhancing ForgeSelect in `crates/forge_select/src/select.rs` to support grouped items with section headers
  - If grouped selection is complex, alternatively pre-format the model list with provider headers as non-selectable items
  - Or use a simpler approach with provider prefix in the display string
  - Rationale: Improves UX by visually organizing models by provider in the selection interface

- [ ] 16. **Add cache statistics and diagnostics**
  - Add methods to report cache statistics like hit/miss rates, number of cached entries, cache memory usage
  - Consider adding a diagnostic command like "forge debug cache" to show cache status
  - Log cache hits and misses at debug level for troubleshooting
  - Rationale: Helps monitor cache effectiveness and diagnose performance issues

- [ ] 17. **Update tests to handle provider_id in Model**
  - Update all model fixtures in tests to include provider_id field
  - Add tests for multi-provider model fetching scenarios
  - Test cache invalidation when credentials change
  - Test TTL expiration and refresh logic
  - Test automatic provider switching when selecting models from different providers
  - Rationale: Ensures the new functionality works correctly and prevents regressions

- [ ] 18. **Handle migration for existing stored models**
  - Check if models are persisted in the database in `crates/forge_repo`
  - If yes, create a migration to add provider_id column to the models table
  - Implement backward compatibility for models without provider_id
  - Consider a data migration script to populate provider_id for existing models based on current provider configuration
  - Rationale: Ensures existing installations can upgrade smoothly without data loss

- [ ] 19. **Update documentation for new model selection behavior**
  - Update user-facing documentation to explain the unified model and provider selection
  - Document the new cache configuration options
  - Add examples of how provider is automatically set when selecting a model
  - Document the cache TTL behavior and how to customize it
  - Rationale: Users need to understand the new behavior and configuration options

## Verification Criteria

- Model struct includes provider_id field and all conversions preserve this information
- Calling get_all_models returns models from all configured LLM providers with provider_id populated
- Models are cached per provider and also in an aggregated cache for multi-provider access
- Cache respects configured TTL and refreshes expired entries appropriately
- Model selection UI displays provider information alongside each model
- Selecting a model from provider A while provider B is active automatically switches to provider A
- The forge list model command shows provider column in output
- Cache is pre-warmed on startup without blocking user interaction
- Configuration options for cache TTL and strategy are respected
- Tests pass including new tests for multi-provider scenarios
- Performance is maintained or improved - model selection is fast due to caching
- Works correctly in both zsh completion scenarios and interactive REPL mode
- Cache invalidation works correctly when credentials are added, updated, or removed

## Potential Risks and Mitigations

1. **Performance degradation when fetching from many providers**
   Mitigation: Use parallel async fetching with tokio join_all to fetch from all providers concurrently. Add timeout configuration to prevent slow providers from blocking the entire operation. Implement aggressive caching with reasonable TTL.

2. **Memory usage increase with multi-provider caching**
   Mitigation: Implement cache size limits and LRU eviction if memory becomes a concern. Monitor cache statistics. Make TTL configurable so users can tune based on their environment. Consider storing Arc references instead of cloning model data.

3. **Breaking changes to Model struct affecting existing code**
   Mitigation: Carefully review all usages of Model struct across the codebase. Update all constructors, builders, and tests. Use derive_setters to provide flexible construction. Implement database migration for any persisted models.

4. **Cache invalidation complexity with multiple providers**
   Mitigation: Clear aggregated cache whenever any provider's credentials change. Keep invalidation logic simple and conservative - when in doubt, invalidate. Log cache operations at debug level for troubleshooting.

5. **UI becoming cluttered with too many models**
   Mitigation: Implement smart sorting and grouping by provider. Consider adding filtering options in the future. Use ForgeSelect's search functionality to help users find models quickly. Show provider headers or prefixes to organize the list.

6. **Race conditions in concurrent cache access**
   Mitigation: Continue using Mutex for service-level caches to ensure thread-safety. Consider upgrading to RwLock if read-heavy workloads show contention. Ensure proper lock ordering to prevent deadlocks.

7. **Inconsistent provider state when auto-switching**
   Mitigation: Update both model and provider atomically in the API layer. Ensure UI state is updated consistently. Add clear user feedback when provider is automatically switched. Consider adding a confirmation prompt if desired.

8. **Backward compatibility with existing configurations**
   Mitigation: Make provider_id optional (Option) initially if needed for migration. Provide sensible defaults for new config options. Test upgrade path from previous version. Document migration steps clearly.

## Alternative Approaches

1. **Separate model and provider selection instead of unified**
   Trade-offs: Simpler implementation, less change to existing code, but requires two-step selection process and doesn't solve the core UX issue. Unified selection is more intuitive.

2. **Fetch models on-demand per provider instead of all at once**
   Trade-offs: Lower initial memory footprint, but slower UX when browsing models. Caching strategy becomes more complex. The all-at-once approach is better for interactive selection.

3. **Use a separate ModelWithProvider struct instead of adding field to Model**
   Trade-offs: Avoids changing core Model domain, but creates inconsistency and requires more wrapper types. Adding field to Model is cleaner and more maintainable.

4. **Implement provider as a property of ModelId instead of Model**
   Trade-offs: Could encode provider in model identifier (e.g., "openai:gpt-4"), but this conflates identity with provider relationship. Separate field is more flexible and clearer.

5. **Create a federated model registry service**
   Trade-offs: More sophisticated architecture with a dedicated service managing models across providers. Higher complexity but better scalability. Overkill for current needs but could be future enhancement.

6. **Use database for model caching instead of in-memory**
   Trade-offs: Persistent cache survives restarts, but adds database overhead and complexity. Current in-memory approach with smart hydration is faster for interactive use. Could be hybrid approach in future.
