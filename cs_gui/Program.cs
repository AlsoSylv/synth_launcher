using Avalonia;
using System;
using System.Threading.Tasks;
using CsBindgen;

namespace cs_gui;

class Program
{
    // Initialization code. Don't use any Avalonia, third-party APIs or any
    // SynchronizationContext-reliant code before AppMain is called: things aren't initialized
    // yet and stuff might break.
    [STAThread]
    public static void Main(string[] args)
    {
        var handle = new SafeNativeMethods();

        var task = handle.get_manifest();

        task.Wait();
        
        Console.WriteLine(handle.get_latest_release());

        for (nuint i = 0; i < handle.manifest_len(); i++)
        {
            Console.WriteLine(handle.get_version_id(i) + " " + handle.get_version_type(i));
        } 

        // BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    // Avalonia configuration, don't remove; also used by visual designer.
    public static AppBuilder BuildAvaloniaApp()
    {
        return AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
    }
}

class SafeNativeMethods
{
    private unsafe ManifestWrapper* _manifest = null;
    
    public Task get_manifest()
    {
        return Task.Run(() =>
        {
            unsafe
            {
                var taskPointer = NativeMethods.get_version_manifest();
                while (!NativeMethods.poll_manifest_task(taskPointer))
                {
                    
                }

                var manifestPointer = NativeMethods.get_manifest_wrapper();
                var value = NativeMethods.get_manifest(taskPointer, manifestPointer);

                switch (value.code)
                {
                    case Code.Success:
                    {
                        _manifest = manifestPointer;
                        break;
                    }
                }
            }
        });
    }

    public bool is_manifest_loaded()
    {
        unsafe
        {
            return _manifest != null;
        }
    }

    public string get_latest_release()
    {
        unsafe
        {
            check_manifest_internal();
            
            var rawString = NativeMethods.get_latest_release(_manifest);
            var str = new ReadOnlySpan<char>(rawString.char_ptr, (int) rawString.len).ToString();

            NativeMethods.free_string_wrapper(rawString);

            return str;
        }
    }

    private void check_manifest_internal()
    {
        unsafe
        {
            if (_manifest == null)
            {
                throw new NullReferenceException("The manifest was not loaded at the time this was called");
            }
        }
    }

    public nuint manifest_len()
    {
        unsafe
        {
            check_manifest_internal();
            
            return NativeMethods.get_manifest_len(_manifest);
        }
    }

    public string get_version_id(nuint index)
    {
        unsafe
        {
            check_manifest_internal();
            
            var rawString = NativeMethods.get_name(_manifest, index);
            var str = new ReadOnlySpan<char>(rawString.char_ptr, (int) rawString.len).ToString();

            NativeMethods.free_string_wrapper(rawString);

            return str;
        }
    }

    public ReleaseType get_version_type(nuint index)
    {
        unsafe
        {
            check_manifest_internal();

            return NativeMethods.get_type(_manifest, index);
        }
    }
}

internal class RustException : Exception
{
    public RustException(NativeReturn value)
    {
        
    }
}
