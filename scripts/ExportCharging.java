// Focused offline export from the user-supplied Acer service. Never executes it.
// @category Research
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.Reference;
import java.io.*;
import java.util.*;

public class ExportCharging extends GhidraScript {
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 1) throw new IllegalArgumentException("Output directory [extra function addresses]");
        Set<Function> selected = new LinkedHashSet<>();
        try (PrintWriter out = new PrintWriter(new File(args[0], "charging-xrefs.txt"))) {
            for (Data data : currentProgram.getListing().getDefinedData(true)) {
                if (monitor.isCancelled()) return;
                Object value = data.getValue();
                if (!(value instanceof String)) continue;
                String s = ((String)value).toLowerCase(Locale.ROOT);
                if (!s.contains("battery") && !s.contains("charging") && !s.contains("calibrat")) continue;
                for (Reference ref : getReferencesTo(data.getMinAddress())) {
                    Function f = getFunctionContaining(ref.getFromAddress());
                    if (f != null) {
                        selected.add(f);
                        out.println(data.getMinAddress() + " -> " + f.getEntryPoint() + " " + value);
                    }
                }
            }
        }
        for (int i=1; i<args.length; i++) {
            Function f = getFunctionContaining(toAddr(args[i]));
            if (f != null) selected.add(f);
        }
        Set<Function> roots = new LinkedHashSet<>(selected);
        for (Function f : roots) {
            selected.addAll(f.getCallingFunctions(monitor));
            selected.addAll(f.getCalledFunctions(monitor));
        }
        DecompInterface decompiler = new DecompInterface();
        try {
            decompiler.openProgram(currentProgram);
            try (PrintWriter out = new PrintWriter(new File(args[0], currentProgram.getName() + ".charging.c"))) {
                for (Function f : selected) {
                    if (monitor.isCancelled()) break;
                    if (f.isExternal() || f.isThunk()) continue;
                    out.println("\n/* " + f.getEntryPoint() + " " + f.getName() + " */");
                    DecompileResults result = decompiler.decompileFunction(f, 30, monitor);
                    if (result.decompileCompleted()) out.println(result.getDecompiledFunction().getC());
                    else out.println("/* Failed: " + result.getErrorMessage() + " */");
                }
            }
        } finally { decompiler.dispose(); }
    }
}
