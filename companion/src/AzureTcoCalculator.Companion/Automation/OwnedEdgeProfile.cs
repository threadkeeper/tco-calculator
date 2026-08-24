using System.IO;

namespace AzureTcoCalculator.Companion.Automation;

public sealed class OwnedEdgeProfile
{
    private const string MarkerName = ".azure-tco-owned-profile";
    private readonly string _root;

    private OwnedEdgeProfile(string root, string path)
    {
        _root = root;
        Path = path;
    }

    public string Path { get; }

    public static OwnedEdgeProfile Create()
    {
        string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        string root = System.IO.Path.Combine(localAppData, "AzureTcoCalculator", "EdgeProfiles");
        Directory.CreateDirectory(root);
        string path = System.IO.Path.Combine(root, Guid.NewGuid().ToString("D"));
        Directory.CreateDirectory(path);
        File.WriteAllText(System.IO.Path.Combine(path, MarkerName), string.Empty);
        return new OwnedEdgeProfile(
            System.IO.Path.GetFullPath(root).TrimEnd(System.IO.Path.DirectorySeparatorChar) + System.IO.Path.DirectorySeparatorChar,
            System.IO.Path.GetFullPath(path));
    }

    public async Task DeleteAsync()
    {
        if (!Directory.Exists(Path))
        {
            return;
        }
        EnsureOwnedTree();
        for (int attempt = 0; attempt < 4; attempt++)
        {
            try
            {
                Directory.Delete(Path, true);
                return;
            }
            catch (IOException) when (attempt < 3)
            {
                await Task.Delay(250 * (attempt + 1)).ConfigureAwait(false);
            }
            catch (UnauthorizedAccessException) when (attempt < 3)
            {
                await Task.Delay(250 * (attempt + 1)).ConfigureAwait(false);
            }
        }
    }

    private void EnsureOwnedTree()
    {
        string normalized = System.IO.Path.GetFullPath(Path);
        if (!normalized.StartsWith(_root, StringComparison.OrdinalIgnoreCase)
            || !File.Exists(System.IO.Path.Combine(normalized, MarkerName)))
        {
            throw new InvalidOperationException("The isolated Edge profile is not owned by this companion.");
        }

        DirectoryInfo profile = new(normalized);
        if ((profile.Attributes & FileAttributes.ReparsePoint) != 0)
        {
            throw new InvalidOperationException("The isolated Edge profile cannot be cleaned safely.");
        }
        Queue<DirectoryInfo> directories = new();
        directories.Enqueue(profile);
        while (directories.Count > 0)
        {
            foreach (FileSystemInfo entry in directories.Dequeue().EnumerateFileSystemInfos())
            {
                if ((entry.Attributes & FileAttributes.ReparsePoint) != 0)
                {
                    throw new InvalidOperationException("The isolated Edge profile cannot be cleaned safely.");
                }
                if (entry is DirectoryInfo directory)
                {
                    directories.Enqueue(directory);
                }
            }
        }
    }
}