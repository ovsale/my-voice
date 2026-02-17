import { MantineProvider } from "@mantine/core";
import "@mantine/core/styles.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import OverlayApp from "./OverlayApp";

// Styles are imported in OverlayApp.tsx via overlay-global.css

const rootElement = document.getElementById("root");
if (!rootElement) {
	throw new Error("Root element not found");
}

createRoot(rootElement).render(
	<StrictMode>
		<MantineProvider defaultColorScheme="dark">
			<OverlayApp />
		</MantineProvider>
	</StrictMode>,
);
