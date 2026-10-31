import { describe, expect, it } from "vitest";

import { GISCUS_ATTRIBUTES } from "@/lib/giscus";

describe("giscus", () => {
  it("uses Hanamori's discussions, mapped by pathname", () => {
    expect(GISCUS_ATTRIBUTES["data-repo"]).toBe("licyk/Hanamori");
    expect(GISCUS_ATTRIBUTES["data-category"]).toBe("Comment");
    expect(GISCUS_ATTRIBUTES["data-mapping"]).toBe("pathname");
    expect(GISCUS_ATTRIBUTES["data-strict"]).toBe("1");
  });
});
