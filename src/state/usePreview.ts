import { useEffect, useRef, useState } from "react";
import { toRadialError, type Backend } from "../lib/backend";
import type { Edits, RadialError } from "../lib/types";

export interface PreviewState {
  url: string | null;
  loading: boolean;
  error: RadialError | null;
}

interface Loaded {
  key: string;
  url: string | null;
  error: RadialError | null;
}

/**
 * Render `path` through the backend preview pipeline (with `edits` applied),
 * debounced so slider drags don't flood the native side. The previous image
 * stays visible while the next renders; object URLs are revoked when replaced
 * or unmounted.
 */
export function usePreview(
  backend: Backend | null,
  path: string | null,
  edits: Edits,
  maxDim: number,
  page = 0,
  delay = 90,
): PreviewState {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const urlRef = useRef<string | null>(null);
  const editsKey = JSON.stringify(edits);
  const key = path ? `${path}|${page}|${editsKey}|${maxDim}` : "";

  useEffect(() => {
    if (!backend || !path) return;
    let alive = true;
    const t = setTimeout(() => {
      backend
        .preview(path, JSON.parse(editsKey) as Edits, maxDim, page)
        .then((blob) => {
          if (!alive) return;
          const url = URL.createObjectURL(blob);
          if (urlRef.current) URL.revokeObjectURL(urlRef.current);
          urlRef.current = url;
          setLoaded({ key, url, error: null });
        })
        .catch((e: unknown) => {
          if (alive) setLoaded((prev) => ({ key, url: prev?.url ?? null, error: toRadialError(e) }));
        });
    }, delay);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [backend, path, page, editsKey, maxDim, delay, key]);

  useEffect(
    () => () => {
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
      urlRef.current = null;
    },
    [],
  );

  if (!backend || !path) return { url: null, loading: false, error: null };
  // Keep showing the last image of the same file (and page) while a new render is pending.
  const samePath = loaded?.key.startsWith(`${path}|${page}|`) ?? false;
  return {
    url: samePath ? (loaded?.url ?? null) : null,
    loading: loaded?.key !== key,
    error: loaded?.key === key ? loaded.error : null,
  };
}
