const { readFileSync } = require("node:fs");
const { writeFile } = require("node:fs/promises");
const { dirname, extname, join } = require("node:path");
const { chromium } = require("playwright");

const usage = "Usage: mermaid-render INPUT.mmd|- OUTPUT.png|OUTPUT.svg";

async function main(args) {
    if (args.length === 1 && args[0] === "--help") {
        console.log(usage);
        return;
    }
    const [input, output] = args;
    const format = output && extname(output).toLowerCase();
    if (args.length !== 2 || ![".png", ".svg"].includes(format)) {
        throw new Error(usage);
    }

    const definition = readFileSync(input === "-" ? 0 : input, "utf8");
    const browser = await chromium.launch();
    try {
        const page = await browser.newPage({ offline: true, deviceScaleFactor: 2 });
        await page.setContent("<main></main>");
        const bundle = join(dirname(require.resolve("mermaid")), "mermaid.min.js");
        await page.addScriptTag({ path: bundle });
        const svg = await page.evaluate(async (source) => {
            await document.fonts.ready;
            const mermaid = window.mermaid;
            mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
            const result = await mermaid.render("diagram", source);
            const container = document.querySelector("main");
            container.innerHTML = result.svg;
            const diagram = container.querySelector("svg");
            const { width, height } = diagram.viewBox.baseVal;
            if (width > 0 && height > 0) {
                diagram.setAttribute("width", width);
                diagram.setAttribute("height", height);
                diagram.style.maxWidth = "none";
            }
            return diagram.outerHTML;
        }, definition);
        if (format === ".svg") {
            await writeFile(output, svg);
        } else {
            await page.locator("main > svg").screenshot({ path: output, type: "png" });
        }
    } finally {
        await browser.close();
    }
}

main(process.argv.slice(2)).catch((error) => {
    console.error(`mermaid-render: ${error.message}`);
    process.exitCode = 1;
});
