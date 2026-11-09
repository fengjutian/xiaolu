export const SITE = {
  name: "小鲁",
  tagline: "系统透镜",
  version: "0.57.0",
  repositoryUrl: "https://github.com/fengjutian/xiaolu",
  releasesUrl: "https://github.com/fengjutian/xiaolu/releases",
  issuesUrl: "https://github.com/fengjutian/xiaolu/issues",
} as const;

export const NAV_ITEMS = [
  { href: "/features/", label: "功能" },
  { href: "/toolbox/", label: "工具箱" },
  { href: "/ai/", label: "AI 解读" },
  { href: "/security/", label: "安全" },
  { href: "/download/", label: "下载" },
] as const;
