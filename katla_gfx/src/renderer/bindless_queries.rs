use super::*;

impl VulkanRenderer {
    /// Get the bindless texture index for a texture handle.
    ///
    /// Returns 0 if the texture isn't registered with bindless.
    pub fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        self.texture_manager.get_bindless_index(handle).unwrap_or(0)
    }

    /// Get the bindless slot index for a texture handle.
    ///
    /// This is an alias for `get_texture_bindless_index()` that returns an Option
    /// instead of defaulting to 0 for unregistered textures.
    ///
    /// # Arguments
    /// * `handle` - The texture handle to query
    ///
    /// # Returns
    /// The bindless slot index if the texture is registered, None otherwise.
    ///
    /// # Example
    /// ```ignore
    /// if let Some(slot) = renderer.get_bindless_slot(texture_handle) {
    ///     println!("Texture is at bindless slot {}", slot);
    /// } else {
    ///     println!("Texture is not registered with bindless system");
    /// }
    /// ```
    pub fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        self.texture_manager.get_bindless_index(handle)
    }

    /// Get the texture handle at a specific bindless slot.
    ///
    /// This is useful for debugging and texture inspection tools to determine
    /// which texture occupies a given slot.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot index
    ///
    /// # Returns
    /// The TextureHandle at that slot, or None if the slot is not registered
    /// or doesn't exist.
    ///
    /// # Example
    /// ```ignore
    /// // Query which texture is in slot 10
    /// if let Some(handle) = renderer.get_texture_at_slot(10) {
    ///     println!("Texture at slot 10: {:?}", handle);
    /// }
    /// ```
    pub fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        self.texture_manager.get_texture_at_slot(slot)
    }

    /// Get all registered texture handles with their bindless slots.
    ///
    /// This returns an iterator over (TextureHandle, slot) pairs for all
    /// textures that have been registered with the bindless system.
    ///
    /// # Example
    /// ```ignore
    /// for (handle, slot) in renderer.iter_bindless_textures() {
    ///     println!("Texture {:?} is at slot {}", handle, slot);
    /// }
    /// ```
    pub fn iter_bindless_textures(&self) -> impl Iterator<Item = (TextureHandle, u32)> + '_ {
        self.texture_manager.iter_bindless_textures()
    }

    /// Get the font atlas bindless texture slot.
    ///
    /// Returns None if the font atlas has not been registered with the
    /// bindless system yet.
    ///
    /// # Example
    /// ```ignore
    /// if let Some(slot) = renderer.get_font_atlas_bindless_slot() {
    ///     println!("Font atlas is at bindless slot {}", slot);
    /// }
    /// ```
    pub fn get_font_atlas_bindless_slot(&self) -> Option<u32> {
        self.ui_renderer.font_atlas_bindless_slot()
    }

    /// Get information about bindless texture slot utilization.
    ///
    /// Returns (occupied_count, available_count, total_count).
    ///
    /// # Example
    /// ```ignore
    /// let (occupied, available, total) = renderer.get_bindless_stats();
    /// println!("Bindless slots: {}/{} used", occupied, total);
    /// ```
    pub fn get_bindless_stats(&self) -> (usize, usize, usize) {
        (
            self.bindless_manager.occupied_slot_count(),
            self.bindless_manager.available_slot_count(),
            self.bindless_manager.total_slot_count(),
        )
    }

    /// Get a debug representation of bindless slot allocation.
    ///
    /// Returns a string showing which slots are occupied and which are free.
    /// Useful for debugging texture allocation issues.
    ///
    /// # Example
    /// ```ignore
    /// let debug_info = renderer.debug_bindless_slot_allocation();
    /// println!("{}", debug_info);
    /// ```
    pub fn debug_bindless_slot_allocation(&self) -> String {
        self.bindless_manager.debug_slot_allocation()
    }

    /// Get a list of all occupied bindless slots.
    ///
    /// Returns a vector of (slot, image_view) pairs for all occupied slots.
    /// Useful for debugging which textures are currently bound.
    ///
    /// # Example
    /// ```ignore
    /// for (slot, image_view) in renderer.list_occupied_bindless_slots() {
    ///     println!("Slot {}: ImageView({:?})", slot, image_view);
    /// }
    /// ```
    pub fn list_occupied_bindless_slots(&self) -> Vec<(u32, ash::vk::ImageView)> {
        self.bindless_manager.list_occupied_slots()
    }

    /// Get debug information about a specific bindless slot.
    ///
    /// Returns a string describing the slot contents.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot index
    ///
    /// # Example
    /// ```ignore
    /// println!("{}", renderer.debug_bindless_slot_info(5));
    /// ```
    pub fn debug_bindless_slot_info(&self, slot: u32) -> String {
        self.bindless_manager.debug_slot_info(slot)
    }

    /// Get a debug representation of all registered bindless textures.
    ///
    /// Returns a string listing all texture handles with their bindless slots.
    /// Useful for debugging texture allocation and slot assignments.
    ///
    /// # Example
    /// ```ignore
    /// let debug_info = renderer.debug_bindless_textures();
    /// println!("{}", debug_info);
    /// ```
    pub fn debug_bindless_textures(&self) -> String {
        self.texture_manager.debug_bindless_textures()
    }

    /// Get a list of all texture handles that are not registered with bindless.
    ///
    /// Returns texture handles that exist but don't have a bindless slot.
    /// Useful for finding textures that should be registered but aren't.
    ///
    /// # Example
    /// ```ignore
    /// for handle in renderer.list_unregistered_textures() {
    ///     println!("Texture {:?} is not registered with bindless", handle);
    /// }
    /// ```
    pub fn list_unregistered_textures(&self) -> Vec<crate::TextureHandle> {
        self.texture_manager.list_unregistered_textures()
    }

    /// Check if a texture is registered with the bindless system.
    ///
    /// # Arguments
    /// * `handle` - The texture handle to check
    ///
    /// # Returns
    /// true if the texture has a bindless slot assigned.
    ///
    /// # Example
    /// ```ignore
    /// if !renderer.is_bindless_registered(texture_handle) {
    ///     println!("Texture is not registered with bindless");
    /// }
    /// ```
    pub fn is_bindless_registered(&self, handle: crate::TextureHandle) -> bool {
        self.texture_manager.is_bindless_registered(handle)
    }

    /// Get bindless texture registration statistics.
    ///
    /// Returns (registered_count, unregistered_count, total_count).
    ///
    /// # Example
    /// ```ignore
    /// let (registered, unregistered, total) = renderer.get_bindless_registration_stats();
    /// println!("Bindless: {}/{} registered", registered, total);
    /// ```
    pub fn get_bindless_registration_stats(&self) -> (usize, usize, usize) {
        self.texture_manager.bindless_stats()
    }
}
