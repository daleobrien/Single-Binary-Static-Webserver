import { useEffect, useRef } from 'react';

/**
 * Dynamically injects / updates a <style> tag in the document <head>.
 * The CSS is replaced whenever `css` changes, and removed on unmount.
 *
 * Requires `ALLOW_INLINE_STYLES=1` at Rust build-time so the CSP includes
 * `'unsafe-inline'` in `style-src`.
 *
 * @param {string} css    - The CSS string to inject.
 * @param {string} [id]   - Optional unique id for the style element (avoids duplicates).
 */
export default function useDynamicStyles(css, id = 'dynamic-styles') {
  const styleRef = useRef(null);

  useEffect(() => {
    let style = document.getElementById(id);

    if (!style) {
      style = document.createElement('style');
      style.id = id;
      document.head.appendChild(style);
    }

    style.textContent = css;
    styleRef.current = style;

    return () => {
      if (styleRef.current && document.head.contains(styleRef.current)) {
        document.head.removeChild(styleRef.current);
        styleRef.current = null;
      }
    };
  }, [css, id]);
}
