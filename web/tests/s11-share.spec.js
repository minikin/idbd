import { expect, test } from "@playwright/test";

// S11: the pages carry what search engines and link previews read, and every file they
// point at is served.

const SITE = "https://idbd.minikin.me";

const PAGES = [
  { path: "/", canonical: `${SITE}/`, type: "WebApplication" },
  { path: "/explorer", canonical: `${SITE}/explorer`, type: "WebPage" },
];

const meta = (page, attr, name) =>
  page.locator(`meta[${attr}="${name}"]`).getAttribute("content");

for (const { path, canonical, type } of PAGES) {
  test(`S11 ${path} has a canonical link, share cards and structured data`, async ({ page, request }) => {
    await page.goto(path);
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute("href", canonical);

    const title = await page.title();
    const description = await meta(page, "name", "description");
    expect(await meta(page, "property", "og:title")).toBe(title);
    expect(await meta(page, "name", "twitter:title")).toBe(title);
    expect(await meta(page, "property", "og:description")).toBe(description);
    expect(await meta(page, "name", "twitter:description")).toBe(description);
    expect(await meta(page, "property", "og:url")).toBe(canonical);
    expect(await meta(page, "name", "twitter:card")).toBe("summary_large_image");

    // Link previews need an absolute image URL. Its path must exist in the site we deploy.
    const image = await meta(page, "property", "og:image");
    expect(image.startsWith(`${SITE}/`)).toBe(true);
    expect(await meta(page, "name", "twitter:image")).toBe(image);
    const imagePath = new URL(image).pathname;
    const size = await page.evaluate(async (src) => {
      const img = new Image();
      img.src = src;
      await img.decode();
      return [img.naturalWidth, img.naturalHeight];
    }, imagePath);
    expect(size).toEqual([1200, 630]);
    expect(await meta(page, "property", "og:image:width")).toBe("1200");
    expect(await meta(page, "property", "og:image:height")).toBe("630");
    expect(await meta(page, "property", "og:image:alt")).toBeTruthy();
    expect(await meta(page, "property", "og:image:type")).toBe("image/jpeg");

    for (const icon of await page.locator('link[rel="icon"], link[rel="apple-touch-icon"], link[rel="manifest"]').all()) {
      const href = await icon.getAttribute("href");
      expect((await request.get(href)).status(), href).toBe(200);
    }

    const ld = JSON.parse(await page.locator('script[type="application/ld+json"]').textContent());
    expect(ld["@type"]).toBe(type);
    expect(ld.url).toBe(canonical);
    // One app identity: the Explorer is a page of the app the story page describes
    const app = type === "WebApplication" ? ld : ld.isPartOf;
    expect(app["@id"]).toBe(`${SITE}/#app`);
    if (type === "WebApplication") expect(ld.isBasedOn.codeRepository).toBe("https://github.com/minikin/idbd");

    // Internal links go straight to canonical addresses, so crawlers never follow a redirect
    const hrefs = await page.locator("a[href]").evaluateAll((links) => links.map((a) => a.getAttribute("href")));
    expect(hrefs.filter((href) => /(^|\/)(index|explorer)\.html$/.test(href))).toEqual([]);
  });
}

test("S11 the sitemap lists every canonical page and robots.txt points to it", async ({ request }) => {
  const manifest = await (await request.get("/site.webmanifest")).json();
  for (const icon of manifest.icons) expect((await request.get(icon.src)).status(), icon.src).toBe(200);
  const sitemap = await (await request.get("/sitemap.xml")).text();
  for (const { canonical } of PAGES) expect(sitemap).toContain(`<loc>${canonical}</loc>`);
  const robots = await (await request.get("/robots.txt")).text();
  expect(robots).toContain(`Sitemap: ${SITE}/sitemap.xml`);
});

for (const { path } of PAGES) {
  test(`S11 ${path} opens other sites in a new tab and links the blog post from its chips`, async ({ page }) => {
    await page.goto(path);
    const external = await page.locator('a[href^="http"]').evaluateAll((links) =>
      links.map((a) => ({ href: a.getAttribute("href"), target: a.target, rel: a.rel })),
    );
    expect(external.length).toBeGreaterThan(0);
    for (const link of external) {
      expect(link.target, link.href).toBe("_blank");
      expect(link.rel, link.href).toContain("noopener");
    }
    // The chips in the hero or header and in the footer
    await expect(page.locator('.project-badge[href="https://minikin.me/blog/idbd-in-rust/"]')).toHaveCount(2);
  });
}
