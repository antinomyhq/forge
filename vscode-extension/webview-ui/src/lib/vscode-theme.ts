/// VSCode theme integration utilities
/// The CSS variables are now directly mapped in index.css
/// This file provides minimal runtime theme detection

/// Initialize VSCode theme - detect if dark mode is active
export function initVSCodeTheme(): void {
  // VSCode applies theme classes to the body element
  // No need to manually set CSS variables anymore since we're using VSCode vars directly
  console.log('[VSCode Theme] Theme initialized - using VSCode CSS variables');
}

/// Watch for VSCode theme changes
export function watchVSCodeTheme(): void {
  // Create a MutationObserver to watch for class changes on body
  // This helps if VSCode dynamically changes themes
  const observer = new MutationObserver(() => {
    console.log('[VSCode Theme] Theme change detected');
  });
  
  observer.observe(document.body, {
    attributes: true,
    attributeFilter: ['class'],
  });
}
