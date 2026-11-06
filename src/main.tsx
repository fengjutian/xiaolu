import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import {
  createTheme,
  MantineProvider,
  type MantineColorScheme,
} from "@mantine/core";
import "@mantine/core/styles.css";
import App from "./App";

const theme = createTheme({
  primaryColor: "indigo",
  fontFamily: "Inter, 'Segoe UI', 'Microsoft YaHei', sans-serif",
  defaultRadius: "md",
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
