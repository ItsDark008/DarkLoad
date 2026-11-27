import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { AudioTracksField } from "@/components/AudioTracksField";
import { DownloadPresets } from "@/components/DownloadPresets";
import { setLocale } from "@/i18n";
import { AUTOMATIC_AUDIO, optionsFromSettings } from "@/lib/options";
import { useApp } from "@/stores/app";
import type { DownloadPreset, MediaFormat, Settings } from "@/types/api";

Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
afterEach(() => {
  cleanup();
  setLocale("en");
});

describe("download customization", () => {
  it("selects the original track and a dubbed track together", () => {
    const formats = [
      { id: "251", acodec: "opus", language: "en", languagePreference: 10 },
      { id: "251-1", acodec: "opus", language: "it" },
    ] as MediaFormat[];
    function Editor() {
      const [selection, setSelection] = useState(AUTOMATIC_AUDIO);
      return <AudioTracksField formats={formats} selection={selection} onChange={setSelection} video />;
    }
    render(<Editor />);
    fireEvent.click(screen.getByText("Audio tracks", { selector: "summary span" }));
    const original = screen.getByRole("checkbox", { name: "Original · English" });
    const italian = screen.getByRole("checkbox", { name: "Italian" });
    fireEvent.click(original);
    fireEvent.click(italian);
    expect(original).toBeChecked();
    expect(italian).toBeChecked();
    expect(screen.getByText("Original, Italian")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "French" })).not.toBeInTheDocument();
  });

  it("filters selectable languages before analysis and preserves tracks across searches", () => {
    function Editor() {
      const [selection, setSelection] = useState(AUTOMATIC_AUDIO);
      return <AudioTracksField selection={selection} onChange={setSelection} video />;
    }
    render(<Editor />);
    fireEvent.click(screen.getByText("Audio tracks", { selector: "summary span" }));
    const search = screen.getByRole("searchbox", { name: "Search languages" });
    fireEvent.change(search, { target: { value: "ITAL" } });
    expect(screen.queryByRole("checkbox", { name: "French" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "Italian" }));
    fireEvent.change(search, { target: { value: "no such language" } });
    expect(screen.getByRole("status")).toHaveTextContent("No matching languages.");
    fireEvent.change(search, { target: { value: "" } });
    expect(screen.getByRole("checkbox", { name: "Italian" })).toBeChecked();
  });

  it("shows the applied preset and restores the defaults with Originale in Italian", async () => {
    const settings = { downloadPresets: [], downloadDir: "downloads", defaultContainer: "mp4", subtitleLanguages: ["en"] } as unknown as Settings;
    const options = { ...optionsFromSettings(settings), container: "mkv" as const, downloadSubtitles: true };
    useApp.setState({ settings, selectedPresets: {} });
    vi.mocked(invoke).mockImplementation(async (_command, args) => (args as { settings: Settings }).settings as never);
    const first = render(<DownloadPresets page="downloader" options={options} outputDir="archive" onApply={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Presets" }));
    fireEvent.change(screen.getByLabelText("Preset name"), { target: { value: "Archive" } });
    fireEvent.click(screen.getByRole("button", { name: "Save preset" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const preset = { name: "Archive", options, outputDir: "archive" };
    expect(invoke).toHaveBeenCalledWith("settings_update", { settings: { ...settings, downloadPresets: [preset] } });
    expect(screen.getByRole("combobox")).toHaveTextContent("Archive");
    first.unmount();
    setLocale("it");

    const defaults: DownloadPreset = { name: "Originale", options: optionsFromSettings(settings), outputDir: null };
    const onApply = vi.fn();
    function BatchPresets() {
      const [current, setCurrent] = useState(defaults);
      return <DownloadPresets page="batch" options={current.options} outputDir={current.outputDir ?? settings.downloadDir} onApply={(choice) => {
        onApply(choice);
        setCurrent(choice);
      }} />;
    }
    const choose = async (name: string) => {
      fireEvent.keyDown(screen.getByRole("combobox"), { key: "Enter" });
      fireEvent.keyDown(await screen.findByRole("option", { name }), { key: "Enter" });
    };
    render(<BatchPresets />);
    expect(screen.getByRole("combobox")).toHaveTextContent("Originale");
    await choose("Archive");
    expect(onApply).toHaveBeenCalledWith(preset);
    expect(screen.getByRole("combobox")).toHaveTextContent("Archive");
    await choose("Originale");
    expect(onApply).toHaveBeenLastCalledWith(defaults);
    expect(screen.getByRole("combobox")).toHaveTextContent("Originale");
  });
});
