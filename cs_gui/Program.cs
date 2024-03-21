using Avalonia;
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using csbindings;

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
                
                    if (v.code != csbindings.Code.Success) throw new RustException(v);
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
                
                    if (v.code != csbindings.Code.Success) throw new RustException(v);
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
                
                if (v.code != csbindings.Code.Success) throw new RustException(v);
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
                
                if (v.code != csbindings.Code.Success) throw new RustException(v);
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
                
                if (v.code != csbindings.Code.Success) throw new RustException(v);
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
                
                if (v.code != csbindings.Code.Success) throw new RustException(v);
            }
        }
    }

    public ulong Total => _total;
    public double Percentage => (double) _finished / _total;
}

public class VersionWrapper: INotifyPropertyChanged
{
    private readonly unsafe State* _state;
    private unsafe VersionErased* _version;
    private bool _selected;

    public bool Selected
    {
        get => _selected; 
        set => SetField(ref _selected, value);
    }

    public VersionWrapper(SafeNativeMethods handle, nuint index)
    {
        unsafe
        {
            _state = handle.State;
            _version = NativeMethods.get_version(_state, index);
        }
    }
    
    public Task GetJson(CancellationToken token) =>
        Task.Run(() => {
            unsafe {
                token.ThrowIfCancellationRequested();
                var versionTaskPointer = NativeMethods.get_version_task(_state, _version);
                while (!NativeMethods.poll_version_task(versionTaskPointer)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_version_task(versionTaskPointer);
                    token.ThrowIfCancellationRequested();
                }

                var value = NativeMethods.await_version_task(_state, versionTaskPointer);
                if (value.code != csbindings.Code.Success) throw new RustException(value);

                if (token.IsCancellationRequested) return;

                var assetIndexTaskPointer = NativeMethods.get_asset_index(_state);
                while (!NativeMethods.poll_asset_index(assetIndexTaskPointer)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_asset_index(assetIndexTaskPointer);
                    token.ThrowIfCancellationRequested();
                }

                var v = NativeMethods.await_asset_index(_state, assetIndexTaskPointer);
                if (v.code != csbindings.Code.Success) throw new RustException(v);
            }
        }, token);

    public string Name
    {
        get
        {
            unsafe
            {
                return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.version_name(_version)));
            }
        }
    }

    public csbindings.ReleaseType Type
    {
        get
        {
            unsafe
            {
                return NativeMethods.version_type(_version);
            }
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    protected virtual void OnPropertyChanged([CallerMemberName] string? propertyName = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }

    private void SetField<T>(ref T field, T value, [CallerMemberName] string? propertyName = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value)) return;
        field = value;
        OnPropertyChanged(propertyName);
    }
}

public class SafeNativeMethods {
    internal readonly unsafe State* State;
    private unsafe LauncherData* _data;

    public SafeNativeMethods() {
        var path = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData).ToCharArray();
        unsafe {
            fixed (char* utf16Ptr = path) {
                State = NativeMethods.new_rust_state(utf16Ptr, (nuint) path.Length);
            }
        }
    }

    public Task GetData() => Task.Run(() => {
        unsafe {
            var taskPtr = NativeMethods.read_data(State);
            while (!NativeMethods.poll_data(taskPtr)) { }

            var ptr = NativeMethods.alloc_data();
            var v = NativeMethods.await_data(taskPtr, ptr);
            if (v.code != Code.Success) {
                throw new RustException(v);
            }

            _data = ptr;
        }
    });
    
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

    public nuint ManifestLength {
        get {
            unsafe {
                return NativeMethods.get_manifest_len(State);
            }
        }
    }

    public ReadOnlySpan<byte> GetVersionId(nuint index) {
        unsafe {
            return Program.CopyRefString(NativeMethods.get_name(State, index));
        }
    }
    
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

                var value = NativeMethods.await_auth_loop(State, _data, taskPointer);

                if (value.code != csbindings.Code.Success) {
                    throw new RustException(value);
                }
            }
        }, token);

    public Task GetDeviceResponse => Task.Run(() => {
        unsafe {
            var responseTask = NativeMethods.get_device_response();
            while (!NativeMethods.poll_device_response(responseTask)) { }

            var response = NativeMethods.await_device_response(State, responseTask);
            if (response.code != csbindings.Code.Success) throw new RustException(response);
        }
    });

    public string AuthCode {
        get {
            unsafe {
                return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_user_code(State)));
            }
        }
    }
    
    public string AuthUrl {
        get {
            unsafe {
                return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_url(State)));
            }
        }
    }

    public nuint AccountLength {
        get {
            unsafe {
                return NativeMethods.accounts_len(_data);
            }
        }
    }

    public string GetAccountName(nuint index) {
        unsafe {
            return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_account_name(_data, index)));
        }
    }

    public Task RefreshAccount(nuint index) => Task.Run(() => {
        unsafe {
            var task = NativeMethods.try_refresh(_data, index);
            while (!NativeMethods.poll_refresh(task)) { }

            var v = NativeMethods.await_refresh(State, _data, task);

            if (v.code != csbindings.Code.Success) throw new RustException(v);
        }
    });

    public bool NeedsRefresh(nuint index) {
        unsafe {
            return NativeMethods.needs_refresh(_data, index);
        }
    }

    public void RemoveAccount(nuint index) {
        unsafe {
            NativeMethods.remove_account(_data, index);
        }
    }

    public nuint JvmLen {
        get {
            unsafe {
                return NativeMethods.jvm_len(_data);
            }
        }
    }
    
    public string GetJvmName(nuint index) {
        unsafe {
            return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.jvm_name(_data, index)));
        }
    }

    public void AddJvm(string path) {
        unsafe {
            var arr = path.ToCharArray();
            fixed (char* str = arr) NativeMethods.add_jvm(_data, (ushort*)str, (nuint)arr.Length);
        }
    }

    public void RemoveJvm(nuint index) {
        unsafe {
            NativeMethods.remove_jvm(_data, index);
        }
    }

    public void Play(nuint jvmIndex, nuint accIndex) {
        unsafe {
            NativeMethods.play(State, _data, jvmIndex, accIndex);
        }
    }
    
    public void Play(nuint accIndex) {
        unsafe {
            NativeMethods.play_default_jvm(State, _data, accIndex);
        }
    }
}

internal class RustException(NativeReturn value)
    : Exception(value.code + " " + "todo");
