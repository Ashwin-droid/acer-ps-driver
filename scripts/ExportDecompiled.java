// Export recovered function bodies for local protocol research.
// @category Research
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.listing.Function;
import java.io.PrintWriter;
import java.io.File;

public class ExportDecompiled extends GhidraScript {
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 1) throw new IllegalArgumentException("Expected output directory");
        DecompInterface decompiler = new DecompInterface();
        try {
            decompiler.openProgram(currentProgram);
            try (PrintWriter out = new PrintWriter(new File(args[0], currentProgram.getName() + ".c"))) {
                for (Function function : currentProgram.getFunctionManager().getFunctions(true)) {
                    if (monitor.isCancelled()) break;
                    if (function.isExternal() || function.isThunk()) continue;
                    out.println("\n/* " + function.getEntryPoint() + " " + function.getName() + " */");
                    DecompileResults result = decompiler.decompileFunction(function, 20, monitor);
                    if (result.decompileCompleted()) out.println(result.getDecompiledFunction().getC());
                    else out.println("/* DECOMPILATION FAILED: " + result.getErrorMessage() + " */");
                }
            }
        } finally {
            decompiler.dispose();
        }
    }
}
