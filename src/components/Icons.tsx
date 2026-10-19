// Inline SVG icons (24×24 grid, 1.7px strokes). Decorative by default;
// the controls that use them carry their own accessible labels.

import type { ReactNode, SVGProps } from "react";
import type { Format, OperationId } from "../lib/types";

type P = SVGProps<SVGSVGElement>;

function Svg({ children, ...p }: P & { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...p}
    >
      {children}
    </svg>
  );
}

export const IconOpen = (p: P) => (
  <Svg {...p}>
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
    <path d="M14 3v5h5M12 11v6M9 14h6" />
  </Svg>
);
export const IconHistory = (p: P) => (
  <Svg {...p}>
    <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
    <path d="M3 3v5h5M12 7v5l3 2" />
  </Svg>
);
export const IconSettings = (p: P) => (
  <Svg {...p}>
    <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
    <circle cx="16" cy="7" r="2" />
    <circle cx="10" cy="17" r="2" />
  </Svg>
);
export const IconClose = (p: P) => (
  <Svg {...p}>
    <path d="M6 6l12 12M18 6L6 18" />
  </Svg>
);
export const IconBack = (p: P) => (
  <Svg {...p}>
    <path d="M15 18l-6-6 6-6" />
  </Svg>
);
export const IconCheck = (p: P) => (
  <Svg {...p}>
    <path d="M5 12.5l4.5 4.5L19 7.5" />
  </Svg>
);
export const IconAlert = (p: P) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7.5v5.5M12 16.5v.01" />
  </Svg>
);
export const IconFolder = (p: P) => (
  <Svg {...p}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
  </Svg>
);
export const IconExternal = (p: P) => (
  <Svg {...p}>
    <path d="M14 4h6v6M20 4l-9 9" />
    <path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4" />
  </Svg>
);
export const IconRotateLeft = (p: P) => (
  <Svg {...p}>
    <path d="M4 4v5h5" />
    <path d="M4.6 9A8 8 0 1 1 6 16.3" />
  </Svg>
);
export const IconRotateRight = (p: P) => (
  <Svg {...p}>
    <path d="M20 4v5h-5" />
    <path d="M19.4 9A8 8 0 1 0 18 16.3" />
  </Svg>
);
export const IconCrop = (p: P) => (
  <Svg {...p}>
    <path d="M6 2v14a2 2 0 0 0 2 2h14" />
    <path d="M2 6h14a2 2 0 0 1 2 2v14" />
  </Svg>
);
export const IconZoomIn = (p: P) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="M20 20l-4-4M11 8v6M8 11h6" />
  </Svg>
);
export const IconZoomOut = (p: P) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="M20 20l-4-4M8 11h6" />
  </Svg>
);
export const IconFit = (p: P) => (
  <Svg {...p}>
    <path d="M4 9V5a1 1 0 0 1 1-1h4M15 4h4a1 1 0 0 1 1 1v4M20 15v4a1 1 0 0 1-1 1h-4M9 20H5a1 1 0 0 1-1-1v-4" />
  </Svg>
);
export const IconActualSize = (p: P) => (
  <Svg {...p}>
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <path d="M8 10v5M7 10.5l1-.5M15 10v5M14 10.5l1-.5M11.5 11v.01M11.5 14v.01" />
  </Svg>
);
export const IconAdjust = (p: P) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M12 3.5v17" />
    <path d="M12 3.5a8.5 8.5 0 0 1 0 17z" fill="currentColor" stroke="none" />
  </Svg>
);
export const IconLink = (p: P) => (
  <Svg {...p}>
    <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" />
    <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
  </Svg>
);
export const IconUnlink = (p: P) => (
  <Svg {...p}>
    <path d="M15.5 13.5l3.2-3.2a4 4 0 0 0-5.7-5.7l-1 1M8.5 10.5l-3.2 3.2a4 4 0 0 0 5.7 5.7l1-1M3 3l18 18" />
  </Svg>
);
export const IconShield = (p: P) => (
  <Svg {...p}>
    <path d="M12 3l7 3v5c0 4.5-3 8.3-7 10-4-1.7-7-5.5-7-10V6z" />
    <path d="M9 12l2 2 4-4" />
  </Svg>
);
export const IconRetry = (p: P) => (
  <Svg {...p}>
    <path d="M20 11a8 8 0 1 0-2.3 5.7" />
    <path d="M20 4v7h-7" />
  </Svg>
);
export const IconArrowRight = (p: P) => (
  <Svg {...p}>
    <path d="M5 12h14M13 6l6 6-6 6" />
  </Svg>
);
export const IconTrash = (p: P) => (
  <Svg {...p}>
    <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" />
  </Svg>
);
export const IconEdit = (p: P) => (
  <Svg {...p}>
    <path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z" />
    <path d="M13.5 6.5l4 4" />
  </Svg>
);
export const IconCompare = (p: P) => (
  <Svg {...p}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M12 4v16" />
  </Svg>
);

/** Recognisable glyph per output format, used in the ring and lists. */
export function FormatIcon({ format, ...p }: P & { format: Format }) {
  switch (format) {
    case "png":
      // Transparency checkerboard.
      return (
        <Svg {...p}>
          <rect x="4" y="4" width="16" height="16" rx="2.5" />
          <path d="M4 9.3h16M4 14.6h16M9.3 4v16M14.6 4v16" strokeOpacity={0.35} />
          <path d="M4.8 4.8h4.5v4.5H4.8zM14.6 4.8h4.6v4.5h-4.6zM9.3 9.3h5.3v5.3H9.3zM4.8 14.6h4.5v4.6H4.8zM14.6 14.6h4.6v4.6h-4.6z" fill="currentColor" stroke="none" opacity={0.55} />
        </Svg>
      );
    case "jpeg":
      // Photograph.
      return (
        <Svg {...p}>
          <rect x="3" y="5" width="18" height="14" rx="2.5" />
          <circle cx="15.5" cy="9.5" r="1.6" />
          <path d="M3.5 17l5-5.5 4 4 2.5-2.5 5.5 5" />
        </Svg>
      );
    case "webp":
      // Globe — the web format.
      return (
        <Svg {...p}>
          <circle cx="12" cy="12" r="8.5" />
          <path d="M3.5 12h17M12 3.5c2.6 2.4 3.8 5.3 3.8 8.5s-1.2 6.1-3.8 8.5c-2.6-2.4-3.8-5.3-3.8-8.5S9.4 5.9 12 3.5z" />
        </Svg>
      );
    case "bmp":
      // Raw pixel grid.
      return (
        <Svg {...p}>
          <path d="M5 5h4v4H5zM10 5h4v4h-4zM15 5h4v4h-4zM5 10h4v4H5zM15 10h4v4h-4zM5 15h4v4H5zM10 15h4v4h-4zM15 15h4v4h-4z" />
          <path d="M10 10h4v4h-4z" fill="currentColor" stroke="none" />
        </Svg>
      );
    case "tiff":
      // Layered archive pages.
      return (
        <Svg {...p}>
          <path d="M12 3.5l8.5 4.5L12 12.5 3.5 8z" />
          <path d="M3.5 12L12 16.5 20.5 12" />
          <path d="M3.5 16L12 20.5 20.5 16" />
        </Svg>
      );
    case "gif":
      // Film strip — frames.
      return (
        <Svg {...p}>
          <rect x="4" y="3.5" width="16" height="17" rx="2" />
          <path d="M8 3.5v17M16 3.5v17M4 8h4M4 12h4M4 16h4M16 8h4M16 12h4M16 16h4" />
        </Svg>
      );
    case "pdf":
      // Document with a folded corner.
      return (
        <Svg {...p}>
          <path d="M6 3.5h8l4.5 4.5v12.5H6z" />
          <path d="M14 3.5V8h4.5" />
          <path d="M8.5 12.5h7M8.5 15.5h7M8.5 18h4" strokeOpacity={0.6} />
        </Svg>
      );
    case "docx":
      // Page with a "W".
      return (
        <Svg {...p}>
          <path d="M6 3.5h8l4.5 4.5v12.5H6z" />
          <path d="M14 3.5V8h4.5" />
          <path d="M8.5 11.5l1.3 5 1.7-4 1.7 4 1.3-5" />
        </Svg>
      );
    case "txt":
      // Lines of text.
      return (
        <Svg {...p}>
          <path d="M5 6h14M5 10h14M5 14h14M5 18h9" />
        </Svg>
      );
  }
}

/** Glyph per operation, used in the ring and the operation list. */
export function OperationIcon({ op, ...p }: P & { op: OperationId }) {
  switch (op) {
    case "convert":
      return (
        <Svg {...p}>
          <path d="M4 8h13l-3-3M20 16H7l3 3" />
        </Svg>
      );
    case "compress":
      return (
        <Svg {...p}>
          <path d="M9 3v6H3M15 3v6h6M9 21v-6H3M15 21v-6h6" />
        </Svg>
      );
    case "edit":
      return (
        <Svg {...p}>
          <path d="M4 7h8M16 7h4M4 17h4M12 17h8" />
          <circle cx="14" cy="7" r="2" />
          <circle cx="10" cy="17" r="2" />
        </Svg>
      );
    case "crop":
      return (
        <Svg {...p}>
          <path d="M6 2v14a2 2 0 0 0 2 2h14" />
          <path d="M2 6h14a2 2 0 0 1 2 2v14" />
        </Svg>
      );
    case "annotate":
      return (
        <Svg {...p}>
          <path d="M4 20l4-1L19 8a2.1 2.1 0 0 0-3-3L5 16z" />
          <path d="M14 7l3 3M4 20h16" strokeOpacity={0.5} />
        </Svg>
      );
    case "redact":
      return (
        <Svg {...p}>
          <rect x="3" y="5" width="18" height="14" rx="2" />
          <path d="M6 10h8M6 14h12" />
          <rect x="9" y="8.6" width="9" height="2.8" fill="currentColor" stroke="none" />
        </Svg>
      );
    case "background":
      return (
        <Svg {...p}>
          <rect x="3" y="3" width="18" height="18" rx="3" fill="currentColor" fillOpacity={0.25} />
          <rect x="7.5" y="7.5" width="9" height="9" rx="1.5" />
        </Svg>
      );
    case "metadata":
      return (
        <Svg {...p}>
          <path d="M3 12V4a1 1 0 0 1 1-1h8l9 9-9 9z" />
          <circle cx="8" cy="8" r="1.5" />
        </Svg>
      );
  }
}
