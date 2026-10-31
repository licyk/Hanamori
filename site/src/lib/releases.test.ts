import { describe, expect, it } from "vitest";
import { parseRelease } from "@/lib/releases";

const asset = {
  name: "hanamori-x86_64-pc-windows-msvc.exe",
  size: 100,
  browser_download_url: "https://github.com/licyk/Hanamori/releases/download/v1.0.0/hanamori-x86_64-pc-windows-msvc.exe",
};
const release = {
  tag_name: "v1.0.0",
  published_at: "2026-01-01T00:00:00Z",
  assets: [asset],
  extra: "ignored",
};

describe("download selection", () => {
  it("offers only actual matching assets and tolerates unknown fields", () => {
    const result = parseRelease({
      ...release,
      assets: [asset, { ...asset, name: "msvc-installer.exe" }],
    });
    expect(result).toEqual({
      status: "available",
      release: {
        version: release.tag_name,
        published: release.published_at,
        url: "https://github.com/licyk/Hanamori/releases/tag/v1.0.0",
        assets: [{ name: asset.name, url: asset.browser_download_url, size: asset.size }],
      },
    });
  });

  it("does not present old or prerelease builds as compatible downloads", () => {
    expect(parseRelease({ ...release, assets: [{ ...asset, name: "msvc-installer.exe" }] }).status)
      .toBe("unavailable");
    expect(parseRelease({ ...release, prerelease: true }).status).toBe("unavailable");
  });

  it("reports malformed responses instead of claiming that no release exists", () => {
    expect(() => parseRelease({})).toThrow();
    expect(() => parseRelease({
      ...release,
      assets: [{ ...asset, browser_download_url: "javascript:alert(1)" }],
    })).toThrow();
  });
});
