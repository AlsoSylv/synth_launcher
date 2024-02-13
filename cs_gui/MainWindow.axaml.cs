using System;
using System.Collections.Generic;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Controls;

namespace cs_gui;

public partial class MainWindow : Window {
    private Task? _versionTask;
    private readonly CancellationTokenSource _token = new();

    public MainWindow() {
        InitializeComponent();

        var task = SafeNativeMethods.GetManifest();

        try {
            task.Wait();

            var list = new List<string>();
            var len = SafeNativeMethods.ManifestLength();

            for (UIntPtr idx = 0; idx < len; idx++) {
                list.Add(Encoding.UTF8.GetString(SafeNativeMethods.GetVersionId(idx)));
            }

            VersionSelectBox.ItemsSource = list;
        }
        catch (AggregateException ae) {
            ae.Handle(x => {
                if (x is not RustException) return false;
                Console.WriteLine(x);
                return true;
            });
        }
    }

    private async void VersionSelectBox_OnSelectionChanged(object? _, SelectionChangedEventArgs e) {
        if (_versionTask == null) {
            var index = VersionSelectBox.SelectedIndex;
            _versionTask = SafeNativeMethods.GetVersion((nuint)index, _token.Token);
        }
        else {
            await _token.CancelAsync();
            _token.TryReset();

            var index = VersionSelectBox.SelectedIndex;
            _versionTask = SafeNativeMethods.GetVersion((nuint)index, _token.Token);
        }
    }
}