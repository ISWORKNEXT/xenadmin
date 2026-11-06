/* Copyright (c) ISWORKNEXT
 *
 * Licensed under the BSD 2-Clause license, the same terms as the rest of
 * this repository.
 */

using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
using XenAdmin.Actions;
using XenAdmin.Core;
using XenAPI;

namespace XenAdmin.SettingsPanels
{
    /// <summary>
    /// Edits VM.xenstore_data. Keys are paths such as vm-data/gateway.
    /// xapi copies a key into the guest xenstore only when it starts with vm-data/.
    /// Clearing a value, or using Remove, drops that entry.
    /// </summary>
    public class VMXenstoreDataEditPage : UserControl, IEditPage
    {
        private const string KeyPrefix = "vm-data/";

        private readonly DataGridView grid;
        private readonly DataGridViewTextBoxColumn keyColumn;
        private readonly DataGridViewTextBoxColumn valueColumn;
        private readonly Button removeButton;
        private readonly Label errorLabel;

        private VM vm;
        private Dictionary<string, string> original = new Dictionary<string, string>();
        private bool loading;

        public VMXenstoreDataEditPage()
        {
            AutoScaleMode = AutoScaleMode.Dpi;
            Dock = DockStyle.Fill;
            DoubleBuffered = true;

            var notes = new TableLayoutPanel
            {
                Dock = DockStyle.Top,
                AutoSize = true,
                AutoSizeMode = AutoSizeMode.GrowAndShrink,
                ColumnCount = 2,
                Padding = new Padding(3),
                Margin = new Padding(0)
            };
            notes.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 22));
            notes.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            var icon = new PictureBox
            {
                Image = Properties.Resources._000_Alert2_h32bit_16,
                SizeMode = PictureBoxSizeMode.CenterImage,
                Size = new Size(16, 16),
                Margin = new Padding(0, 3, 0, 0)
            };
            var note = new Label
            {
                AutoSize = true,
                Margin = new Padding(3, 3, 3, 6),
                Text = Messages.VM_XENSTORE_DATA_REBOOT + Environment.NewLine
                    + Messages.VM_XENSTORE_DATA_DELETE + Environment.NewLine
                    + "Enter the name only, for example gateway. It is stored as vm-data/gateway."
            };
            notes.Controls.Add(icon, 0, 0);
            notes.Controls.Add(note, 1, 0);

            var buttons = new FlowLayoutPanel
            {
                Dock = DockStyle.Bottom,
                AutoSize = true,
                FlowDirection = FlowDirection.LeftToRight,
                WrapContents = false,
                Padding = new Padding(0, 6, 0, 0)
            };
            var addButton = new Button
            {
                AutoSize = true,
                Text = Messages.VM_XENSTORE_DATA_ADD,
                UseVisualStyleBackColor = true
            };
            removeButton = new Button
            {
                AutoSize = true,
                Text = Messages.VM_XENSTORE_DATA_REMOVE,
                UseVisualStyleBackColor = true,
                Enabled = false
            };
            addButton.Click += AddButton_Click;
            removeButton.Click += RemoveButton_Click;
            buttons.Controls.Add(addButton);
            buttons.Controls.Add(removeButton);

            errorLabel = new Label
            {
                AutoSize = true,
                Dock = DockStyle.Bottom,
                ForeColor = Color.DarkRed,
                Visible = false,
                Padding = new Padding(0, 4, 0, 0)
            };

            keyColumn = new DataGridViewTextBoxColumn
            {
                Name = "key",
                HeaderText = "Key",
                AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill,
                FillWeight = 45,
                SortMode = DataGridViewColumnSortMode.NotSortable
            };
            valueColumn = new DataGridViewTextBoxColumn
            {
                Name = "value",
                HeaderText = "Value",
                AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill,
                FillWeight = 55,
                SortMode = DataGridViewColumnSortMode.NotSortable
            };
            grid = new DataGridView
            {
                Dock = DockStyle.Fill,
                AllowUserToAddRows = false,
                AllowUserToDeleteRows = false,
                AllowUserToResizeRows = false,
                MultiSelect = false,
                SelectionMode = DataGridViewSelectionMode.FullRowSelect,
                RowHeadersVisible = false,
                AutoSizeRowsMode = DataGridViewAutoSizeRowsMode.AllCells,
                BackgroundColor = SystemColors.Window,
                BorderStyle = BorderStyle.Fixed3D,
                EditMode = DataGridViewEditMode.EditOnEnter
            };
            grid.Columns.Add(keyColumn);
            grid.Columns.Add(valueColumn);
            grid.DefaultCellStyle.WrapMode = DataGridViewTriState.True;
            grid.SelectionChanged += (sender, args) => UpdateRemoveButton();
            grid.CellEndEdit += Grid_CellEndEdit;
            grid.CellValueChanged += (sender, args) =>
            {
                if (!loading)
                    HideLocalValidationMessages();
            };

            Controls.Add(grid);
            Controls.Add(errorLabel);
            Controls.Add(buttons);
            Controls.Add(notes);
        }

        public sealed override string Text => Messages.VM_XENSTORE_DATA;

        public string SubText
        {
            get
            {
                Dictionary<string, string> entries;
                if (TryReadEntries(out entries, out _) && entries.Count > 0)
                    return string.Format(Messages.VM_XENSTORE_DATA_COUNT, entries.Count);
                return Messages.VM_XENSTORE_DATA_EMPTY;
            }
        }

        public Image Image => Images.StaticImages._002_Configure_h32bit_16;

        public AsyncAction SaveSettings()
        {
            Dictionary<string, string> entries;
            if (vm == null || !TryReadEntries(out entries, out _))
                return null;

            // Clone() shares the dictionary with the cached VM. Replace it.
            vm.xenstore_data = entries;
            return null;
        }

        public void SetXenObjects(IXenObject orig, IXenObject clone)
        {
            vm = clone as VM;
            original = CopyMap(vm == null ? null : vm.xenstore_data);
            Repopulate();
        }

        public bool ValidToSave
        {
            get
            {
                if (!HasChanged)
                    return true;
                Dictionary<string, string> entries;
                string error;
                return TryReadEntries(out entries, out error);
            }
        }

        public void ShowLocalValidationMessages()
        {
            Dictionary<string, string> entries;
            string error;
            TryReadEntries(out entries, out error);
            errorLabel.Text = string.IsNullOrEmpty(error) ? Messages.VM_XENSTORE_DATA_INVALID : error;
            errorLabel.Visible = true;
        }

        public void HideLocalValidationMessages()
        {
            errorLabel.Visible = false;
            errorLabel.Text = string.Empty;
        }

        public void Cleanup()
        {
        }

        public bool HasChanged
        {
            get
            {
                Dictionary<string, string> entries;
                string error;
                if (!TryReadEntries(out entries, out error))
                    return true;
                return !Helper.AreEqual2(original, entries);
            }
        }

        private void Repopulate()
        {
            loading = true;
            HideLocalValidationMessages();
            grid.Rows.Clear();

            foreach (var pair in original.OrderBy(pair => pair.Key, StringComparer.Ordinal))
            {
                if (IsPlaceholderKey(pair.Key))
                    continue;
                grid.Rows.Add(DisplayKey(pair.Key), pair.Value ?? string.Empty);
            }

            UpdateRemoveButton();
            loading = false;
        }

        private void AddButton_Click(object sender, EventArgs e)
        {
            var index = grid.Rows.Add(string.Empty, string.Empty);
            grid.CurrentCell = grid.Rows[index].Cells[keyColumn.Index];
            grid.BeginEdit(true);
            HideLocalValidationMessages();
        }

        private void RemoveButton_Click(object sender, EventArgs e)
        {
            if (grid.CurrentRow == null)
                return;

            var index = grid.CurrentRow.Index;
            grid.Rows.RemoveAt(index);
            UpdateRemoveButton();
            HideLocalValidationMessages();
        }

        private void UpdateRemoveButton()
        {
            removeButton.Enabled = grid.CurrentRow != null && !grid.CurrentRow.IsNewRow;
        }

        private void Grid_CellEndEdit(object sender, DataGridViewCellEventArgs e)
        {
            if (loading || e.RowIndex < 0 || e.ColumnIndex != keyColumn.Index)
                return;

            var cell = grid.Rows[e.RowIndex].Cells[keyColumn.Index];
            var normalized = NormalizeKey(cell.Value as string);
            if (IsPlaceholderKey(normalized))
            {
                var rowIndex = e.RowIndex;
                BeginInvoke(new Action(() =>
                {
                    if (rowIndex >= 0 && rowIndex < grid.Rows.Count)
                        grid.Rows.RemoveAt(rowIndex);
                    UpdateRemoveButton();
                }));
                return;
            }

            if (normalized.Length > 0 && normalized != (cell.Value as string ?? string.Empty).Trim())
                cell.Value = normalized;
        }

        private bool TryReadEntries(out Dictionary<string, string> entries, out string error)
        {
            entries = new Dictionary<string, string>();
            error = null;

            foreach (DataGridViewRow row in grid.Rows)
            {
                if (row.IsNewRow)
                    continue;

                var key = KeyForSave(row.Cells[keyColumn.Index].Value as string);
                var value = row.Cells[valueColumn.Index].Value as string ?? string.Empty;
                if (IsPlaceholderKey(key) || (key.Length == 0 && value.Length == 0))
                    continue;
                if (value.Length == 0)
                    continue;

                if (!IsValidKey(key))
                {
                    error = Messages.VM_XENSTORE_DATA_INVALID;
                    entries = null;
                    return false;
                }

                if (entries.ContainsKey(key))
                {
                    error = Messages.VM_XENSTORE_DATA_INVALID;
                    entries = null;
                    return false;
                }

                entries[key] = value;
            }

            return true;
        }

        private string KeyForSave(string key)
        {
            key = NormalizeKey(key);
            if (key.Length == 0 || original.ContainsKey(key))
                return key;
            return KeyPrefix + key;
        }

        private static string DisplayKey(string key)
        {
            return NormalizeKey(key);
        }

        private static string NormalizeKey(string key)
        {
            key = (key ?? string.Empty).Trim().Trim('/');
            while (key.StartsWith(KeyPrefix, StringComparison.Ordinal))
                key = key.Substring(KeyPrefix.Length).TrimStart('/');
            if (string.Equals(key, "vm-data", StringComparison.Ordinal))
                return string.Empty;
            return key;
        }

        private static bool IsPlaceholderKey(string key)
        {
            key = (key ?? string.Empty).Trim().TrimEnd('/');
            return key.Length == 0
                || string.Equals(key, "vm-data", StringComparison.Ordinal)
                || string.Equals(key, "vm-data/vm-data", StringComparison.Ordinal);
        }

        private static bool IsValidKey(string key)
        {
            if (IsPlaceholderKey(key) || key == KeyPrefix)
                return false;
            if (!key.StartsWith(KeyPrefix, StringComparison.Ordinal))
                return false;
            if (key.IndexOf('\0') >= 0 || key.IndexOf('\r') >= 0 || key.IndexOf('\n') >= 0)
                return false;
            if (key.EndsWith("/") || key.Contains("//"))
                return false;
            return true;
        }

        private static Dictionary<string, string> CopyMap(Dictionary<string, string> source)
        {
            var copy = new Dictionary<string, string>();
            if (source == null)
                return copy;

            foreach (var pair in source)
            {
                if (IsPlaceholderKey(pair.Key))
                    continue;
                copy[pair.Key] = pair.Value ?? string.Empty;
            }
            return copy;
        }
    }
}
