import EditorPane from "@/components/EditorPane";

const DEMO_FILES = [
  {
    path: "welcome.ts",
    content: "// Welcome to NeuralForge\nexport function hello(): string {\n  return \"hello\";\n}\n",
  },
  {
    path: "notes.md",
    content: "# Notes\n\nThis tab exists to verify tab switching + language modes.\n",
  },
];

export default function Home() {
  return (
    <main className="flex h-screen w-screen flex-col">
      <EditorPane initialFiles={DEMO_FILES} />
    </main>
  );
}
