import { defineConfig } from "vitepress";
import llmstxt from "vitepress-plugin-llms";

export default defineConfig({
  vite: {
    plugins: [llmstxt()],
  },
  title: "scarab",
  description: "config & template expertise — generic config loading and templated content packs",
  base: "/scarab/",
  srcExclude: ["README.md"],
  ignoreDeadLinks: [/\/api\//],
  themeConfig: {
    nav: [
      { text: "Getting Started", link: "/getting-started" },
      { text: "Explanation", link: "/explanation/architecture" },
      { text: "Reference", link: "/reference/config-port" },
      {
        text: "LLMs",
        items: [
          { text: "llms.txt", link: "https://9aia.com/scarab/llms.txt", target: "_blank" },
          {
            text: "llms-full.txt",
            link: "https://9aia.com/scarab/llms-full.txt",
            target: "_blank",
          },
        ],
      },
    ],
    sidebar: [
      {
        text: "Getting Started",
        items: [{ text: "Load a typed config", link: "/getting-started" }],
      },
      {
        text: "How-to",
        items: [
          { text: "Overview", link: "/how-to/" },
          { text: "Customize config discovery", link: "/how-to/customize-config-discovery" },
          { text: "Layer configs from several files", link: "/how-to/layered-config" },
          { text: "Write a template pack", link: "/how-to/write-a-template-pack" },
          { text: "Deploy a template pack", link: "/how-to/deploy-a-template-pack" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Overview", link: "/reference/" },
          { text: "Config port", link: "/reference/config-port" },
          { text: "Template packs", link: "/reference/template-packs" },
          { text: "Template driver", link: "/reference/driver" },
          { text: "Testkit", link: "/reference/testkit" },
        ],
      },
      {
        text: "Explanation",
        items: [
          { text: "Overview", link: "/explanation/" },
          { text: "Architecture", link: "/explanation/architecture" },
          { text: "Boundaries", link: "/explanation/boundaries" },
        ],
      },
    ],
    socialLinks: [{ icon: "github", link: "https://github.com/9aia/scarab" }],
  },
});
