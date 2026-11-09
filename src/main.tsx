import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import {
  createTheme,
  MantineProvider,
  type MantineColorsTuple,
  type MantineColorScheme,
} from "@mantine/core";
import "@mantine/core/styles.css";
import App from "./App";

const verdant: MantineColorsTuple = [
  "#e9f8f2",
  "#d4f0e6",
  "#a9dfcd",
  "#79cdb2",
  "#50bc9b",
  "#36b18d",
  "#25a982",
  "#158c70",
  "#08715b",
  "#005d4a",
];

const theme = createTheme({
  colors: { verdant },
  primaryColor: "verdant",
  primaryShade: { light: 7, dark: 5 },
  fontFamily: "Inter, 'Segoe UI', 'Microsoft YaHei', sans-serif",
  defaultRadius: "md",
  defaultGradient: { from: "verdant.7", to: "lime.7", deg: 115 },
});

const currentHour = new Date().getHours();
const defaultColorScheme: MantineColorScheme =
  currentHour >= 7 && currentHour < 19 ? "light" : "dark";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MantineProvider theme={theme} defaultColorScheme={defaultColorScheme}>
      <App />
    </MantineProvider>
  </StrictMode>,
);
