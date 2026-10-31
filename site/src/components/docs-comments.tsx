import { useEffect, useRef } from "react";

import { GISCUS_ATTRIBUTES, GISCUS_ORIGIN } from "@/lib/giscus";
import { useSiteCopy } from "@/lib/site-copy";

export function DocsComments() {
  const containerRef = useRef<HTMLDivElement>(null);
  const copy = useSiteCopy().docs;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const script = document.createElement("script");
    script.src = `${GISCUS_ORIGIN}/client.js`;
    script.async = true;
    script.crossOrigin = "anonymous";
    for (const [name, value] of Object.entries(GISCUS_ATTRIBUTES)) {
      script.setAttribute(name, value);
    }
    container.append(script);
    return () => container.replaceChildren();
  }, []);

  return (
    <section aria-label={copy.comments} className="mt-12 border-t pt-8">
      <div ref={containerRef} />
    </section>
  );
}
