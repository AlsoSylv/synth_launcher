using Avalonia;
using System;
using System.Text;
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
        
        Console.Write(handle.get_latest_release());

        for (nuint i = 0; i < handle.manifest_len(); i++)
        {
            Console.WriteLine(handle.get_version_id(i));
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
    private readonly unsafe LauncherPointer* _launcher;
    private unsafe ManifestWrapper* _manifest;

    public SafeNativeMethods()
    {
        unsafe
        {
            _launcher = NativeMethods.new_launcher();
        }
    }

    public Task get_manifest()
    {
        return Task.Run(() =>
        {
            unsafe
            {
                var task = NativeMethods.get_version_manifest(this._launcher);
                while (!NativeMethods.poll_manifest_task(task))
                {
                    
                }

                _manifest = NativeMethods.get_manifest(task);
            }
        });
    }

    public string get_latest_release()
    {
        unsafe
        {
            var rawString = NativeMethods.get_latest_release(_manifest);
            var builder = new StringBuilder();
            for (nuint i = 0; i < rawString.len; i++)
            {
                builder.Append(Convert.ToChar(*(rawString.char_ptr + i)));
            }

            return builder.ToString();
        }
    }

    public nuint manifest_len()
    {
        unsafe
        {
            return NativeMethods.get_manifest_len(_manifest);
        }
    }

    public string get_version_id(nuint index)
    {
        unsafe
        {
            var rawString = NativeMethods.get_name(_manifest, index);
            var builder = new StringBuilder();
            for (nuint i = 0; i < rawString.len; i++)
            {
                builder.Append(Convert.ToChar(*(rawString.char_ptr + i)));
            }

            return builder.ToString();
        }
    }
}
