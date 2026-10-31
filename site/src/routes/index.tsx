import { createFileRoute } from "@tanstack/react-router";
import { HomeLayout } from "fumadocs-ui/layouts/home";
import { useRef } from "react";

import FeaturesSection from "@/components/home/features-section";
import HeroSection from "@/components/home/hero-section";
import HighlightsSection from "@/components/home/highlights-section";
import InstallSection from "@/components/home/install-section";
import WorkflowSection from "@/components/home/workflow-section";
import SectionRail from "@/components/page/section-rail";
import { useDocumentMetadata } from "@/lib/document-metadata";
import { HOME_SECTION_IDS, homeSections } from "@/lib/home-sections";
import type { Locale } from "@/lib/i18n";
import { baseOptions } from "@/lib/layout.shared";
import { useActiveSection, useScrollReveal } from "@/lib/page-motion";
import { useSiteLocale } from "@/lib/site-copy";

const METADATA: Record<Locale, { description: string; title: string }> = {
  "zh-CN": {
    title: "Hanamori — 独立 MSVC 工具链管理",
    description:
      "独立安装 MSVC 与 Windows SDK，按项目选择工具链，在终端中编译和构建。",
  },
  en: {
    title: "Hanamori — Standalone MSVC toolchain manager",
    description:
      "Install independent MSVC and Windows SDK combinations, select toolchains per project and build from your terminal.",
  },
};

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: METADATA["zh-CN"].title },
      { name: "description", content: METADATA["zh-CN"].description },
    ],
  }),
  component: Home,
});

function Home() {
  const locale = useSiteLocale();
  const homeRef = useRef<HTMLElement>(null);
  const sections = homeSections(locale);
  const activeSectionId = useActiveSection(HOME_SECTION_IDS);

  useDocumentMetadata(METADATA[locale]);
  useScrollReveal(homeRef);

  return (
    <HomeLayout {...baseOptions(locale)} className="home-page paged" ref={homeRef}>
      <SectionRail activeId={activeSectionId} sections={sections} />
      <HeroSection />
      <FeaturesSection />
      <WorkflowSection />
      <HighlightsSection />
      <InstallSection />
    </HomeLayout>
  );
}
