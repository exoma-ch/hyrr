import "./lib/styles/tokens.css";
import Viewer from "./Viewer.svelte";
import { mount } from "svelte";

const app = mount(Viewer, {
  target: document.getElementById("app")!,
});

export default app;
