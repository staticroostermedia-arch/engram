// Updated adapter with navigation hooks (sweeps + attractor now "walk" the flat map)

impl IsomorphicFlatEarthMap {
    // Hook: Navigation now walks the flat map using existing primitives
    pub fn navigate_on_flat_map(&self, from: DiscPosition, to: DiscPosition) -> Result<PlanSequence, NavigationError> {
        // 1. Use existing sweeps on the disc geometry
        let swept_paths = self.sweeps.generate(from, to);
        // 2. Apply attractor for optimal trajectory fixed point
        let optimal = self.attractor.settle(swept_paths, self.current_reward_bias);
        // 3. Integrate with schema_bind and predictive_map_sr
        let bound = self.schema_bind(optimal);
        Ok(PlanSequence { path: optimal, provenance: "flat_map_walk" })
    }

    // Example usage with previous mappings
    pub fn example_walk_2020_to_contract_law(&self) {
        self.map_2020_events();  // Anchor on disc
        self.map_contract_law(); // Anchor on stars
        self.navigate_on_flat_map(DiscPosition::Sector("northwest_quarter"), DiscPosition::StarCluster("sirius_cluster"));
    }
}

// Geosphere translation still included for Aric compatibility
