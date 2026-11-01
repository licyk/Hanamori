export const RELEASES_URL = "https://github.com/licyk/Hanamori/releases";
export const RELEASE_API = "https://api.github.com/repos/licyk/Hanamori/releases/latest";
const ASSETS = [
  "hanamori-x86_64-pc-windows-msvc.exe",
  "hanamori-x86_64-pc-windows-msvc.zip",
  "hanamori-aarch64-pc-windows-msvc.exe",
  "hanamori-aarch64-pc-windows-msvc.zip",
] as const;

export interface ReleaseAsset {
  name: string;
  url: string;
  size: number;
}

export interface Release {
  version: string;
  published: string;
  url: string;
  assets: ReleaseAsset[];
}

export type ReleaseResult =
  | { status: "available"; release: Release }
  | { status: "unavailable" };

export function parseRelease(value: unknown): ReleaseResult {
  if (typeof value !== "object" || value === null) throw new Error("Invalid release response");
  const data = value as Record<string, unknown>;
  if (data.draft === true || data.prerelease === true) return { status: "unavailable" };
  if (
    typeof data.tag_name !== "string" ||
    typeof data.published_at !== "string" ||
    !Array.isArray(data.assets)
  ) {
    throw new Error("Incomplete release response");
  }

  const assets: ReleaseAsset[] = [];
  for (const value of data.assets) {
    if (typeof value !== "object" || value === null) continue;
    const asset = value as Record<string, unknown>;
    if (!ASSETS.some(name => name === asset.name)) continue;
    if (
      typeof asset.browser_download_url !== "string" ||
      typeof asset.size !== "number" ||
      !Number.isFinite(asset.size) || asset.size <= 0
    ) {
      throw new Error("Incomplete download asset");
    }
    const url = new URL(asset.browser_download_url);
    if (url.origin !== "https://github.com" || !url.pathname.startsWith("/licyk/Hanamori/releases/download/")) {
      throw new Error("Unexpected download URL");
    }
    assets.push({ name: asset.name as string, url: url.href, size: asset.size });
  }

  if (!assets.length) return { status: "unavailable" };
  return {
    status: "available",
    release: {
      version: data.tag_name,
      published: data.published_at,
      url: `${RELEASES_URL}/tag/${encodeURIComponent(data.tag_name)}`,
      assets,
    },
  };
}

export async function fetchRelease(signal?: AbortSignal): Promise<ReleaseResult> {
  const response = await fetch(RELEASE_API, {
    headers: { Accept: "application/vnd.github+json" },
    signal,
  });
  // A private repository and a repository with no published release both return 404.
  if (response.status === 404) return { status: "unavailable" };
  if (!response.ok) throw new Error(`GitHub HTTP ${response.status}`);
  return parseRelease(await response.json());
}
