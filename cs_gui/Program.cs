using Avalonia;
using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using CsBindgen;

namespace cs_gui;

internal static class Program {
    // Initialization code. Don't use any Avalonia, third-party APIs or any
    // SynchronizationContext-reliant code before AppMain is called: things aren't initialized
    // yet and stuff might break.
    [STAThread]
    public static void Main(string[] args) {
        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    // Avalonia configuration, don't remove; also used by visual designer.
    private static AppBuilder BuildAvaloniaApp() {
        return AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
    }

    public static ReadOnlySpan<byte> CopyRefString(RefStringWrapper wrapper) {
        unsafe {
            return new ReadOnlySpan<byte>(wrapper.char_ptr, (int)wrapper.len);
        }
    }

    public static string CopyAndFreeOwnedString(OwnedStringWrapper wrapper) {
        unsafe {
            var str = Encoding.UTF8.GetString(wrapper.char_ptr, (int)wrapper.len);
            NativeMethods.free_owned_string_wrapper(wrapper);
            return str;
        }
    }
}

public class AssetTask {
    // TODO: This needs to be replaced with Rust atomics and FFI calls to function on ARM correctly 
    private ulong _total;
    private ulong _finished;
    public Task Task { get; private set; }

    public AssetTask(SafeNativeMethods state) {
        Task = Task.Run(delegate
        {
            unsafe
            {
                fixed (ulong* total = &_total, finished = &_finished) {
                    var assetTask = NativeMethods.get_assets(state.State, total, finished);
                    while (!NativeMethods.poll_assets(assetTask)) { }

                    var v = NativeMethods.await_assets(assetTask);
                
                    if (v.code != Code.Success) throw new RustException(v);
                }
            }
        });
    }

    public AssetTask(SafeNativeMethods state, CancellationToken token) {
        Task = Task.Run(delegate
        {
            unsafe
            {
                fixed (ulong* total = &_total, finished = &_finished) {
                    var assetTask = NativeMethods.get_assets(state.State, total, finished);
                    while (!NativeMethods.poll_assets(assetTask)) {
                        if (!token.IsCancellationRequested) continue;
                        NativeMethods.cancel_assets(assetTask);
                        token.ThrowIfCancellationRequested();
                    }

                    var v = NativeMethods.await_assets(assetTask);
                
                    if (v.code != Code.Success) throw new RustException(v);
                }
            }
        }, token);
    }

    public ulong Total => _total;
    public double Percentage => (double) _finished / _total;
}

public class LibrariesTask {
    private ulong _total;
    private ulong _finished;
    private readonly unsafe State* _state;
    public Task Task { get; private set; }

    public LibrariesTask(ref SafeNativeMethods state) {
        unsafe {
            _state = state.State;
        }
        Task = Task.Run(GetLibraries);
        return;

        unsafe void GetLibraries() {
            fixed (ulong* total = &_total, finished = &_finished) {
                var assetTask = NativeMethods.get_libraries(_state, total, finished);
                while (!NativeMethods.poll_libraries(assetTask)) { }

                var v = NativeMethods.await_libraries(_state, assetTask);
                
                if (v.code != Code.Success) throw new RustException(v);
            }
        }
    }

    public LibrariesTask(ref SafeNativeMethods state, CancellationToken token) {
        unsafe {
            _state = state.State;
        }
        Task = Task.Run(GetLibraries, token);
        return;

        unsafe void GetLibraries() {
            fixed (ulong* total = &_total, finished = &_finished) {
                var assetTask = NativeMethods.get_libraries(_state, total, finished);
                while (!NativeMethods.poll_libraries(assetTask)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_libraries(assetTask);
                    token.ThrowIfCancellationRequested();
                }

                var v = NativeMethods.await_libraries(_state, assetTask);
                
                if (v.code != Code.Success) throw new RustException(v);
            }
        }
    }

    public ulong Total => _total;
    public double Percentage => (double) _finished / _total;
}

public class JarTask {
    private ulong _total;
    private ulong _finished;
    private readonly unsafe State* _state;
    public Task Task { get; private set; }

    public JarTask(ref SafeNativeMethods state) {
        unsafe {
            _state = state.State;
        }
        Task = Task.Run(GetJar);
        return;

        unsafe void GetJar() {
            fixed (ulong* total = &_total, finished = &_finished) {
                var assetTask = NativeMethods.get_jar(_state, total, finished);
                while (!NativeMethods.poll_jar(assetTask)) { }

                var v = NativeMethods.await_jar(_state, assetTask);
                
                if (v.code != Code.Success) throw new RustException(v);
            }
        }
    }

    public JarTask(ref SafeNativeMethods state, CancellationToken token) {
        unsafe {
            _state = state.State;
        }
        Task = Task.Run(GetJar, token);
        return;

        unsafe void GetJar() {
            fixed (ulong* total = &_total, finished = &_finished) {
                var assetTask = NativeMethods.get_jar(_state, total, finished);
                while (!NativeMethods.poll_jar(assetTask)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_jar(assetTask);
                    token.ThrowIfCancellationRequested();
                }

                var v = NativeMethods.await_jar(_state, assetTask);
                
                if (v.code != Code.Success) throw new RustException(v);
            }
        }
    }

    public ulong Total => _total;
    public double Percentage => (double) _finished / _total;
}

public class SafeNativeMethods {
    internal readonly unsafe State* State;

    public SafeNativeMethods() {
        var path = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData).ToCharArray();
        unsafe {
            fixed (char* utf16Ptr = path) {
                State = NativeMethods.new_rust_state((ushort*) utf16Ptr, (nuint) path.Length);
            }
        }
    }
    
    public Task GetManifest() =>
        Task.Run(() => {
            unsafe {
                var taskPointer = NativeMethods.get_version_manifest(State);
                while (!NativeMethods.poll_manifest_task(taskPointer)) { }

                var value = NativeMethods.await_version_manifest(State, taskPointer);

                if (value.code != Code.Success) {
                    throw new RustException(value);
                }
            }
        });

    public bool IsManifestNull {
        get {
            unsafe {
                return NativeMethods.is_manifest_null(State);
            }
        }
    }

    /// <summary>
    /// TODO: This needs to return an index, not a name
    /// </summary>
    /// <returns></returns>
    public ReadOnlySpan<byte> GetLatestRelease() {
        unsafe {
            return Program.CopyRefString(NativeMethods.get_latest_release(State));
        }
    }

    public nuint ManifestLength() {
        unsafe {
            return NativeMethods.get_manifest_len(State);
        }
    }

    public ReadOnlySpan<byte> GetVersionId(nuint index) {
        unsafe {
            return Program.CopyRefString(NativeMethods.get_name(State, index));
        }
    }

    public ReleaseType GetVersionType(nuint index) {
        unsafe {
            return NativeMethods.get_type(State, index);
        }
    }

    public Task GetVersion(nuint index, CancellationToken token) =>
        Task.Run(() => {
            unsafe {
                token.ThrowIfCancellationRequested();
                var versionTaskPointer = NativeMethods.get_version_task(State, index);
                while (!NativeMethods.poll_version_task(versionTaskPointer)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_version_task(versionTaskPointer);
                    token.ThrowIfCancellationRequested();
                }

                var value = NativeMethods.await_version_task(State, versionTaskPointer);
                if (value.code != Code.Success) throw new RustException(value);

                if (token.IsCancellationRequested) return;

                var assetIndexTaskPointer = NativeMethods.get_asset_index(State);
                while (!NativeMethods.poll_asset_index(assetIndexTaskPointer)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_asset_index(assetIndexTaskPointer);
                    token.ThrowIfCancellationRequested();
                }

                var v = NativeMethods.await_asset_index(State, assetIndexTaskPointer);
                if (v.code != Code.Success) throw new RustException(v);
            }
        }, token);
    
    public Task Auth(CancellationToken token) =>
        Task.Run(() => {
            unsafe {
                if (token.IsCancellationRequested) token.ThrowIfCancellationRequested();
                var taskPointer = NativeMethods.start_auth_loop(State);
                while (!NativeMethods.poll_auth_loop(taskPointer)) {
                    try
                    {
                        Task.Delay(100, token);
                    }
                    catch (Exception)
                    {
                        NativeMethods.cancel_auth_loop(taskPointer);
                        throw;
                    }
                }

                if (token.IsCancellationRequested)
                {
                    NativeMethods.cancel_auth_loop(taskPointer);
                    token.ThrowIfCancellationRequested();
                }

                var value = NativeMethods.await_auth_loop(State, taskPointer);

                if (value.code != Code.Success) {
                    throw new RustException(value);
                }
            }
        }, token);

    public Task GetDeviceResponse => Task.Run(() => {
        unsafe {
            var responseTask = NativeMethods.get_device_response();
            while (!NativeMethods.poll_device_response(responseTask)) { }

            var response = NativeMethods.await_device_response(State, responseTask);
            if (response.code != Code.Success) throw new RustException(response);
        }
    });

    public string GetCode() {
        unsafe {
            return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_user_code(State)));
        }
    }
    
    public string GetUrl() {
        unsafe {
            return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_url(State)));
        }
    }

    public nuint AccountLength {
        get {
            unsafe {
                return NativeMethods.accounts_len(State);
            }
        }
    }

    public string GetAccountName(nuint index) {
        unsafe {
            return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_account_name(State, index)));
        }
    }

    public Task RefreshAccount(nuint index) => Task.Run(() => {
        unsafe {
            var task = NativeMethods.try_refresh(State, index);
            while (!NativeMethods.poll_refresh(task)) { }

            var v = NativeMethods.await_refresh(State, task);

            if (v.code != Code.Success) throw new RustException(v);
        }
    });

    public bool NeedsRefresh(nuint index) {
        unsafe {
            return NativeMethods.needs_refresh(State, index);
        }
    }

    public void RemoveAccount(nuint index) {
        unsafe {
            NativeMethods.remove_account(State, index);
        }
    }

    public nuint JvmLen {
        get {
            unsafe {
                return NativeMethods.jvm_len(State);
            }
        }
    }
    
    public string GetJvmName(nuint index) {
        unsafe {
            return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.jvm_name(State, index)));
        }
    }

    public void AddJvm(string path) {
        unsafe {
            var arr = path.ToCharArray();
            fixed (char* str = arr) NativeMethods.add_jvm(State, (ushort*)str, (nuint)arr.Length);
        }
    }

    public void Play(nuint jvmIndex, nuint accIndex) {
        unsafe {
            NativeMethods.play(State, jvmIndex, accIndex);
        }
    }
    
    public void Play(nuint accIndex) {
        unsafe {
            NativeMethods.play_default_jvm(State, accIndex);
        }
    }
}

internal class RustException(NativeReturn value)
    : Exception(value.code + " " + Program.CopyAndFreeOwnedString(value.error));
