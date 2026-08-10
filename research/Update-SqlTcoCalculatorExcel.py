from __future__ import annotations

import math
from pathlib import Path

import pywintypes
import win32com.client as win32


WORKBOOK_PATH = Path(__file__).with_name("SQL TCO Calculator.xlsx").resolve()

SQL_PRICING_URL = (
    "https://cdn-dynmedia-1.microsoft.com/is/content/microsoftcorp/microsoft/"
    "bade/documents/products-and-services/en-us/cloud/SQL-Server-2025-Pricing.pdf"
)
AZURE_PRICING_API_URL = (
    "https://prices.azure.com/api/retail/prices?currencyCode=USD&$filter="
    "serviceName%20eq%20%27Azure%20Arc%20Enabled%20Databases%27%20and%20"
    "productName%20eq%20%27Azure%20Arc-enabled%20SQL%20Server%20-%20"
    "Arc-enabled%20servers%27"
)
LICENSING_RULES_URL = (
    "https://learn.microsoft.com/en-us/sql/sql-server/azure-arc/"
    "manage-license-billing?view=sql-server-ver17"
)

ENTERPRISE_PERPETUAL = 15_123.0
STANDARD_PERPETUAL = 3_945.0
STANDARD_SERVER = 989.0
STANDARD_CAL = 230.0
ENTERPRISE_ANNUAL = 5_434.0
STANDARD_ANNUAL = 1_418.0
ENTERPRISE_PAYGO = 0.375
STANDARD_PAYGO = 0.100
ENTERPRISE_PAYGO_MONTHLY = 274.0
STANDARD_PAYGO_MONTHLY = 73.0
VERIFIED_DATE = "2026-08-07"

XL_CENTER = -4108
XL_LEFT = -4131
XL_LANDSCAPE = 2
XL_AUTOMATIC = -4105
XL_CONTINUOUS = 1
XL_THIN = 2
XL_VALIDATE_DECIMAL = 2
XL_VALIDATE_WHOLE_NUMBER = 1
XL_VALID_ALERT_STOP = 1
XL_BETWEEN = 1


def ole_color(hex_color: str) -> int:
    value = hex_color.lstrip("#")
    red = int(value[0:2], 16)
    green = int(value[2:4], 16)
    blue = int(value[4:6], 16)
    return red + green * 256 + blue * 65_536


DARK_GRAY = ole_color("4F4F4F")
BLUE = ole_color("1F5F99")
TEAL = ole_color("0086A8")
LIGHT_BLUE = ole_color("DCEFF7")
LIGHT_GRAY = ole_color("F2F2F2")
YELLOW = ole_color("FFF2CC")
GREEN = ole_color("E2F0D9")
RED = ole_color("FCE4D6")
WHITE = ole_color("FFFFFF")
BORDER_COLOR = ole_color("B7B7B7")


def numeric(value: object, fallback: float = 0.0) -> float:
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return float(value)
    return fallback


def set_borders(cell_range) -> None:
    cell_range.Borders.LineStyle = XL_CONTINUOUS
    cell_range.Borders.Color = BORDER_COLOR
    cell_range.Borders.Weight = XL_THIN


def style_range(
    cell_range,
    *,
    fill: int | None = None,
    font_color: int | None = None,
    bold: bool | None = None,
    font_size: int | None = None,
    horizontal: int | None = None,
    wrap: bool | None = None,
    borders: bool = False,
) -> None:
    if fill is not None:
        cell_range.Interior.Color = fill
    if font_color is not None:
        cell_range.Font.Color = font_color
    if bold is not None:
        cell_range.Font.Bold = bold
    if font_size is not None:
        cell_range.Font.Size = font_size
    if horizontal is not None:
        cell_range.HorizontalAlignment = horizontal
    if wrap is not None:
        cell_range.WrapText = wrap
    cell_range.VerticalAlignment = XL_CENTER
    if borders:
        set_borders(cell_range)


def merge_with_value(worksheet, address: str, value: object) -> None:
    cell_range = worksheet.Range(address)
    cell_range.Merge()
    cell_range.Cells(1, 1).Value2 = value


def remove_name(workbook, name: str) -> None:
    try:
        workbook.Names.Item(name).Delete()
    except pywintypes.com_error:
        pass


def set_name(workbook, name: str, refers_to: str) -> None:
    remove_name(workbook, name)
    workbook.Names.Add(name, refers_to)


def add_source_link(worksheet, row: int, label: str, url: str, description: str) -> None:
    worksheet.Cells(row, 1).Value2 = label
    link_cell = worksheet.Cells(row, 2)
    worksheet.Hyperlinks.Add(link_cell, url, "", description, "Open Microsoft source")
    worksheet.Range(f"C{row}:G{row}").Merge()
    worksheet.Cells(row, 3).Value2 = description


def build_price_sheet(workbook, business_case):
    for worksheet in list(workbook.Worksheets):
        if worksheet.Name == "SQL License Book Prices":
            worksheet.Delete()
            break

    price_sheet = workbook.Worksheets.Add(None, business_case)
    price_sheet.Name = "SQL License Book Prices"
    price_sheet.Tab.Color = TEAL
    price_sheet.Activate()
    price_sheet.Application.ActiveWindow.DisplayGridlines = False

    merge_with_value(price_sheet, "A1:G1", "SQL Server 2025 Standard Book Prices")
    style_range(
        price_sheet.Range("A1:G1"),
        fill=BLUE,
        font_color=WHITE,
        bold=True,
        font_size=16,
        horizontal=XL_LEFT,
    )
    price_sheet.Rows(1).RowHeight = 28

    merge_with_value(
        price_sheet,
        "A2:G2",
        "Public Microsoft USD pricing. Customer EA pricing can differ and is modeled separately on the Business Case sheet.",
    )
    style_range(price_sheet.Range("A2:G2"), fill=LIGHT_BLUE, wrap=True)
    price_sheet.Range("A2:G2").Font.Italic = True
    price_sheet.Rows(2).RowHeight = 32

    headers = (
        "Edition",
        "Offer",
        "Licensing unit",
        "Book price (USD)",
        "Billing basis",
        "Channel / effective date",
        "Notes",
    )
    for column, header in enumerate(headers, start=1):
        price_sheet.Cells(4, column).Value2 = header
    style_range(
        price_sheet.Range("A4:G4"),
        fill=DARK_GRAY,
        font_color=WHITE,
        bold=True,
        horizontal=XL_CENTER,
        wrap=True,
        borders=True,
    )

    price_rows = (
        (5, "Enterprise", "Perpetual license", "2-core pack", ENTERPRISE_PERPETUAL, "One-time", "Volume licensing, hosting", "Open no-level estimated retail price"),
        (6, "Standard", "Perpetual license - per core", "2-core pack", STANDARD_PERPETUAL, "One-time", "Volume licensing, hosting", "Open no-level estimated retail price"),
        (7, "Standard", "Server license", "Server", STANDARD_SERVER, "One-time", "Volume licensing, hosting", "CALs required for users or devices"),
        (8, "Standard", "Client access license", "CAL", STANDARD_CAL, "One-time", "Volume licensing, hosting", "One CAL per user or device"),
        (9, "Developer", "Standard and Enterprise", "Not applicable", 0.0, "Free", "Free download", "Non-production use"),
        (10, "Express", "Express edition", "Not applicable", 0.0, "Free", "Free download", "Subject to Express edition limits"),
        (13, "Enterprise", "Subscription / add-on", "2-core pack", ENTERPRISE_ANNUAL, "Annual", "Volume licensing", "Public annual book price"),
        (14, "Standard", "Subscription / add-on", "2-core pack", STANDARD_ANNUAL, "Annual", "Volume licensing", "Public annual book price"),
        (17, "Enterprise", "Azure Arc PAYG", "Core-hour", ENTERPRISE_PAYGO, "Hourly", "Global meter - effective 2022-12-01", "Active Microsoft Retail Prices API rate"),
        (18, "Standard", "Azure Arc PAYG", "Core-hour", STANDARD_PAYGO, "Hourly", "Global meter - effective 2022-12-01", "Active Microsoft Retail Prices API rate"),
        (19, "Enterprise", "Azure Arc PAYG", "Core-month reference", ENTERPRISE_PAYGO_MONTHLY, "Monthly", "SQL Server 2025 pricing PDF", "Published rounded monthly rate"),
        (20, "Standard", "Azure Arc PAYG", "Core-month reference", STANDARD_PAYGO_MONTHLY, "Monthly", "SQL Server 2025 pricing PDF", "Published rounded monthly rate"),
    )
    for row_data in price_rows:
        row_number = row_data[0]
        for column, value in enumerate(row_data[1:], start=1):
            price_sheet.Cells(row_number, column).Value2 = value

    sections = {
        12: "Annual subscription and add-on pricing",
        16: "Azure Arc-enabled SQL Server PAYG pricing",
        22: "Derived first-year license plus annual add-on reference",
    }
    for row_number, label in sections.items():
        merge_with_value(price_sheet, f"A{row_number}:G{row_number}", label)
        style_range(
            price_sheet.Range(f"A{row_number}:G{row_number}"),
            fill=LIGHT_BLUE,
            bold=True,
            wrap=True,
            borders=True,
        )

    derived_rows = (
        (23, "Enterprise", "Perpetual license + annual add-on", "2-core pack", "=D5+D13", "Derived first year", "Calculated from public prices", "Reference only"),
        (24, "Standard", "Perpetual license + annual add-on", "2-core pack", "=D6+D14", "Derived first year", "Calculated from public prices", "Reference only"),
    )
    for row_data in derived_rows:
        row_number = row_data[0]
        for column, value in enumerate(row_data[1:], start=1):
            if column == 4:
                price_sheet.Cells(row_number, column).Formula = value
            else:
                price_sheet.Cells(row_number, column).Value2 = value

    style_range(price_sheet.Range("A5:G24"), wrap=True, borders=True)
    price_sheet.Range("D5:D14").NumberFormat = "$#,##0.00"
    price_sheet.Range("D17:D18").NumberFormat = "$0.000"
    price_sheet.Range("D19:D24").NumberFormat = "$#,##0.00"

    merge_with_value(price_sheet, "A26:G26", "Sources and Assumptions")
    style_range(
        price_sheet.Range("A26:G26"),
        fill=TEAL,
        font_color=WHITE,
        bold=True,
        wrap=True,
        borders=True,
    )
    add_source_link(price_sheet, 27, "SQL Server 2025 pricing", SQL_PRICING_URL, "Official Microsoft SQL Server 2025 pricing PDF")
    add_source_link(price_sheet, 28, "Azure PAYG meter prices", AZURE_PRICING_API_URL, "Microsoft Azure Retail Prices API - Global USD")
    add_source_link(price_sheet, 29, "Licensing and billing rules", LICENSING_RULES_URL, "Microsoft Learn - Azure Arc SQL licensing and billing")
    price_sheet.Cells(30, 1).Value2 = "Verified"
    price_sheet.Cells(30, 2).Value2 = VERIFIED_DATE
    price_sheet.Cells(31, 1).Value2 = "Currency / tax"
    price_sheet.Cells(31, 2).Value2 = "USD; taxes excluded"
    price_sheet.Cells(32, 1).Value2 = "PAYG metering rule"
    price_sheet.Range("B32:G32").Merge()
    price_sheet.Cells(32, 2).Value2 = "Four-core minimum per OSE; billed hourly while SQL Server runs and remains connected."
    price_sheet.Cells(33, 1).Value2 = "EA discount"
    price_sheet.Range("B33:G33").Merge()
    price_sheet.Cells(33, 2).Value2 = "Customer-specific. The initial Business Case value is inferred from the supplied subscription totals."
    style_range(price_sheet.Range("A27:G33"), wrap=True, borders=True)
    style_range(price_sheet.Range("A27:A33"), fill=LIGHT_GRAY, bold=True)
    price_sheet.Rows(32).RowHeight = 32
    price_sheet.Rows(33).RowHeight = 32

    merge_with_value(price_sheet, "A35:G35", "Accuracy Review of Supplied TCO Example")
    style_range(
        price_sheet.Range("A35:G35"),
        fill=TEAL,
        font_color=WHITE,
        bold=True,
        wrap=True,
        borders=True,
    )
    review_headers = (
        "Metric",
        "Example value",
        "Current public value",
        "Change / implied discount",
        "Finding",
        "Verified",
        "Notes",
    )
    for column, header in enumerate(review_headers, start=1):
        price_sheet.Cells(36, column).Value2 = header
    style_range(
        price_sheet.Range("A36:G36"),
        fill=DARK_GRAY,
        font_color=WHITE,
        bold=True,
        horizontal=XL_CENTER,
        wrap=True,
        borders=True,
    )
    review_rows = (
        (37, "Enterprise Arc PAYG / core-hour", 0.39, ENTERPRISE_PAYGO, "=C37/B37-1", "Out of date", VERIFIED_DATE, "Example is 4.0% above the active meter."),
        (38, "Standard Arc PAYG / core-hour", 0.0925, STANDARD_PAYGO, "=C38/B38-1", "Out of date", VERIFIED_DATE, "Example is 7.5% below the active meter."),
        (39, "Enterprise annual subscription / 2-core", 2_982.96, ENTERPRISE_ANNUAL, "=1-B39/C39", "EA/negotiated, not book", VERIFIED_DATE, "Example implies a customer discount."),
        (40, "Standard annual subscription / 2-core", 777.96, STANDARD_ANNUAL, "=1-B40/C40", "EA/negotiated, not book", VERIFIED_DATE, "Example implies a customer discount."),
        (41, "Azure PAYGO discount", 0.10, 0.0, "=B41-C41", "Commercial assumption", VERIFIED_DATE, "Now a separate editable Business Case input."),
    )
    for row_data in review_rows:
        row_number = row_data[0]
        for column, value in enumerate(row_data[1:], start=1):
            if column == 4:
                price_sheet.Cells(row_number, column).Formula = value
            else:
                price_sheet.Cells(row_number, column).Value2 = value
    style_range(price_sheet.Range("A37:G41"), wrap=True, borders=True)
    price_sheet.Range("B37:C38").NumberFormat = "$0.0000"
    price_sheet.Range("B39:C40").NumberFormat = "$#,##0.00"
    price_sheet.Range("D37:D41").NumberFormat = "0.00%"
    price_sheet.Range("B41:C41").NumberFormat = "0.00%"

    widths = (27, 34, 20, 21, 23, 30, 43)
    for column, width in enumerate(widths, start=1):
        price_sheet.Columns(column).ColumnWidth = width
    price_sheet.Range("A1:G41").Font.Name = "Calibri"
    price_sheet.Range("A1:G41").VerticalAlignment = XL_CENTER
    price_sheet.Range("A4:G24").AutoFilter()
    price_sheet.Application.ActiveWindow.SplitRow = 4
    price_sheet.Application.ActiveWindow.FreezePanes = True
    price_sheet.Application.ActiveWindow.Zoom = 90
    try:
        price_sheet.PageSetup.PrintArea = "$A$1:$G$41"
        price_sheet.PageSetup.Orientation = XL_LANDSCAPE
        price_sheet.PageSetup.Zoom = False
        price_sheet.PageSetup.FitToPagesWide = 1
        price_sheet.PageSetup.FitToPagesTall = 2
    except pywintypes.com_error:
        pass

    set_name(workbook, "SQL_EE_Perpetual_2Core", "='SQL License Book Prices'!$D$5")
    set_name(workbook, "SQL_SE_Perpetual_2Core", "='SQL License Book Prices'!$D$6")
    set_name(workbook, "SQL_SE_Server", "='SQL License Book Prices'!$D$7")
    set_name(workbook, "SQL_SE_CAL", "='SQL License Book Prices'!$D$8")
    set_name(workbook, "SQL_EE_Annual_2Core", "='SQL License Book Prices'!$D$13")
    set_name(workbook, "SQL_SE_Annual_2Core", "='SQL License Book Prices'!$D$14")
    set_name(workbook, "SQL_EE_PAYG_CoreHour", "='SQL License Book Prices'!$D$17")
    set_name(workbook, "SQL_SE_PAYG_CoreHour", "='SQL License Book Prices'!$D$18")
    set_name(workbook, "SQL_EE_FirstYear_LSA", "='SQL License Book Prices'!$D$23")
    set_name(workbook, "SQL_SE_FirstYear_LSA", "='SQL License Book Prices'!$D$24")
    return price_sheet


def update_business_case(workbook, worksheet) -> dict[str, float]:
    if worksheet.Range("H4").Value2 == "EA SQL license discount":
        ea_discount = numeric(worksheet.Range("J4").Value2)
        paygo_discount = numeric(worksheet.Range("J5").Value2, 0.10)
        switch_hours = numeric(worksheet.Range("J6").Value2, 400.0)
        existing_paygo_hours = numeric(worksheet.Range("J7").Value2, 730.0)
    else:
        current_subscription_cost = numeric(worksheet.Range("G9").Value2) + numeric(
            worksheet.Range("G10").Value2
        )
        subscription_book_cost = (
            numeric(worksheet.Range("B9").Value2) * STANDARD_ANNUAL
            + numeric(worksheet.Range("B10").Value2) * ENTERPRISE_ANNUAL
        )
        ea_discount = (
            1.0 - current_subscription_cost / subscription_book_cost
            if subscription_book_cost > 0
            else 0.0
        )
        paygo_discount = numeric(worksheet.Range("G19").Value2, 0.10)
        switch_hours = numeric(worksheet.Range("E20").Value2, 400.0)
        existing_paygo_hours = 730.0

    counts = {
        row_number: numeric(worksheet.Range(f"B{row_number}").Value2)
        for row_number in range(5, 13)
    }

    worksheet.Range("H2:K78").UnMerge()
    worksheet.Range("H2:K78").Clear()
    worksheet.Range("G31:J39").Clear()
    worksheet.Activate()
    worksheet.Application.ActiveWindow.DisplayGridlines = False

    merge_with_value(worksheet, "H2:J2", "EDITABLE ASSUMPTIONS")
    style_range(
        worksheet.Range("H2:J2"),
        fill=TEAL,
        font_color=WHITE,
        bold=True,
        horizontal=XL_CENTER,
        wrap=True,
        borders=True,
    )
    labels = {
        4: "EA SQL license discount",
        5: "Azure PAYGO discount",
        6: "Switch PAYGO hours / month",
        7: "Existing PAYGO hours / month",
        8: "Break-even PAYGO discount",
        9: "Difference at selected discount",
        10: "PAYGO result",
    }
    for row_number, label in labels.items():
        merge_with_value(worksheet, f"H{row_number}:I{row_number}", label)
        style_range(
            worksheet.Range(f"H{row_number}:I{row_number}"),
            fill=LIGHT_GRAY,
            bold=True,
            wrap=True,
            borders=True,
        )

    worksheet.Range("J4").Value2 = ea_discount
    worksheet.Range("J5").Value2 = paygo_discount
    worksheet.Range("J6").Value2 = switch_hours
    worksheet.Range("J7").Value2 = existing_paygo_hours
    worksheet.Range("J8").Formula = (
        "=IFERROR(1-(SUM($G$5:$G$10)/(($F$22*12)-"
        "(($B$11*SQL_EE_PAYG_CoreHour+$B$12*SQL_SE_PAYG_CoreHour)*"
        "Existing_PAYGO_Hours_Per_Month*12))),0)"
    )
    worksheet.Range("J9").Formula = "=$G$25"
    worksheet.Range("J10").Formula = (
        '=IF(J8<0,"NO DISCOUNT NEEDED",IF(J8>1,"NO FEASIBLE DISCOUNT",'
        'IF(ABS(J9)<1,"BREAK EVEN",IF(J9>0,"PAYGO HIGHER","PAYGO LOWER"))))'
    )
    style_range(worksheet.Range("J4:J7"), fill=YELLOW, bold=True, horizontal=XL_CENTER, borders=True)
    style_range(worksheet.Range("J8"), fill=LIGHT_BLUE, bold=True, horizontal=XL_CENTER, borders=True)
    style_range(worksheet.Range("J9:J10"), fill=GREEN, bold=True, horizontal=XL_CENTER, borders=True)
    worksheet.Range("J4:J5").NumberFormat = "0.00%"
    worksheet.Range("J6:J7").NumberFormat = "0"
    worksheet.Range("J8").NumberFormat = "0.00%"
    worksheet.Range("J9").NumberFormat = "$#,##0"

    for cell_address in ("J4", "J5"):
        validation = worksheet.Range(cell_address).Validation
        validation.Delete()
        validation.Add(
            XL_VALIDATE_DECIMAL,
            XL_VALID_ALERT_STOP,
            XL_BETWEEN,
            0,
            1,
        )
        validation.IgnoreBlank = False
        validation.InputTitle = ""
        validation.InputMessage = ""
        validation.ErrorTitle = "Invalid discount"
        validation.ErrorMessage = "Use a percentage from 0% to 100%."
        validation.ShowInput = False
        validation.ShowError = True
    for cell_address in ("J6", "J7"):
        validation = worksheet.Range(cell_address).Validation
        validation.Delete()
        validation.Add(
            XL_VALIDATE_WHOLE_NUMBER,
            XL_VALID_ALERT_STOP,
            XL_BETWEEN,
            0,
            744,
        )
        validation.IgnoreBlank = False
        validation.InputTitle = ""
        validation.InputMessage = ""
        validation.ErrorTitle = "Invalid monthly hours"
        validation.ErrorMessage = "Use a whole number from 0 to 744."
        validation.ShowInput = False
        validation.ShowError = True

    merge_with_value(worksheet, "H12:I12", "Pricing reference")
    style_range(worksheet.Range("H12:I12"), fill=LIGHT_GRAY, bold=True, wrap=True, borders=True)
    worksheet.Hyperlinks.Add(
        worksheet.Range("J12"),
        "",
        "'SQL License Book Prices'!A1",
        "Open the sourced SQL Server price sheet",
        "Open price sheet",
    )
    style_range(worksheet.Range("J12"), horizontal=XL_CENTER, borders=True)

    set_name(workbook, "EA_SQL_Discount", "='Business Case'!$J$4")
    set_name(workbook, "Azure_PAYGO_Discount", "='Business Case'!$J$5")
    set_name(workbook, "PAYGO_Hours_Per_Month", "='Business Case'!$J$6")
    set_name(workbook, "Existing_PAYGO_Hours_Per_Month", "='Business Case'!$J$7")
    set_name(workbook, "PAYGO_BreakEven_Discount", "='Business Case'!$J$8")

    worksheet.Range("D4").Value2 = "Monthly equivalent"
    worksheet.Range("E4").Value2 = "Yearly / first year"
    worksheet.Range("F4").Value2 = "Yearly per core"
    worksheet.Range("G4").Value2 = "Extended price"

    annual_formulas = {
        5: "=SQL_EE_Annual_2Core*(1-EA_SQL_Discount)",
        6: "=SQL_SE_Annual_2Core*(1-EA_SQL_Discount)",
        7: "=SQL_SE_FirstYear_LSA*(1-EA_SQL_Discount)",
        8: "=SQL_EE_FirstYear_LSA*(1-EA_SQL_Discount)",
        9: "=SQL_SE_Annual_2Core*(1-EA_SQL_Discount)",
        10: "=SQL_EE_Annual_2Core*(1-EA_SQL_Discount)",
    }
    for row_number, annual_formula in annual_formulas.items():
        worksheet.Range(f"D{row_number}").Formula = f"=E{row_number}/12"
        worksheet.Range(f"E{row_number}").Formula = annual_formula
        worksheet.Range(f"F{row_number}").Formula = f"=E{row_number}/2"
        worksheet.Range(f"G{row_number}").Formula = f"=B{row_number}*E{row_number}"

    worksheet.Range("A11").Value2 = "On Azure PAYG SQL IP EE vCores"
    worksheet.Range("A12").Value2 = "On Azure PAYG SQL IP Std vCores"
    worksheet.Range("C11:C12").Value2 = "Azure PAYG"
    worksheet.Range("D11").Formula = "=SQL_EE_PAYG_CoreHour*Existing_PAYGO_Hours_Per_Month*(1-Azure_PAYGO_Discount)"
    worksheet.Range("E11").Formula = "=D11*12"
    worksheet.Range("F11").Formula = "=E11"
    worksheet.Range("G11").Formula = "=B11*F11"
    worksheet.Range("D12").Formula = "=SQL_SE_PAYG_CoreHour*Existing_PAYGO_Hours_Per_Month*(1-Azure_PAYGO_Discount)"
    worksheet.Range("E12").Formula = "=D12*12"
    worksheet.Range("F12").Formula = "=E12"
    worksheet.Range("G12").Formula = "=B12*F12"
    worksheet.Range("G13").Formula = "=SUM(G5:G12)"
    worksheet.Range("G15").Formula = "=G13"
    worksheet.Range("D5:G12").NumberFormat = "$#,##0.00"

    worksheet.Range("A19").Value2 = "PAYGO"
    worksheet.Range("B19").Value2 = "Licensed cores"
    worksheet.Range("C19").Value2 = "Book rate / core-hour"
    worksheet.Range("E19").Value2 = "Hours / month"
    worksheet.Range("F19").Value2 = "Gross / month"
    worksheet.Range("G19").Value2 = "Net / month"
    worksheet.Range("B20").Formula = "=B5*2+B8*2+B10*2+B11"
    worksheet.Range("B21").Formula = "=B6*2+B7*2+B9*2+B12"
    worksheet.Range("C20").Formula = "=SQL_EE_PAYG_CoreHour"
    worksheet.Range("C21").Formula = "=SQL_SE_PAYG_CoreHour"
    worksheet.Range("E20:E21").Formula = "=PAYGO_Hours_Per_Month"
    worksheet.Range("F20").Formula = "=B20*C20*E20"
    worksheet.Range("F21").Formula = "=B21*C21*E21"
    worksheet.Range("G20").Formula = "=F20*(1-Azure_PAYGO_Discount)"
    worksheet.Range("G21").Formula = "=F21*(1-Azure_PAYGO_Discount)"
    worksheet.Range("B22").Formula = "=SUM(B20:B21)"
    worksheet.Range("F22").Formula = "=SUM(F20:F21)"
    worksheet.Range("G22").Formula = "=SUM(G20:G21)"
    worksheet.Range("C20:C21").NumberFormat = "$0.000"
    worksheet.Range("F20:G22").NumberFormat = "$#,##0"

    worksheet.Range("C24:F27").UnMerge()
    worksheet.Range("C24:F27").ClearContents()
    result_rows = {
        24: ("Annual PAYGO", "=G22*12", "$#,##0"),
        25: ("Difference vs billed", "=G24-G15", "$#,##0"),
        26: ("Break-even discount", "=PAYGO_BreakEven_Discount", "0.00%"),
        27: ("Gross annual PAYGO", "=F22*12", "$#,##0"),
    }
    for row_number, (label, formula, number_format) in result_rows.items():
        worksheet.Range(f"F{row_number}").Value2 = label
        worksheet.Range(f"G{row_number}").Formula = formula
        style_range(worksheet.Range(f"F{row_number}:G{row_number}"), bold=True, borders=True)
        worksheet.Range(f"G{row_number}").NumberFormat = number_format
    worksheet.Range("G24").Interior.Color = YELLOW
    worksheet.Range("G25").Interior.Color = LIGHT_BLUE
    worksheet.Range("G26").Interior.Color = GREEN
    worksheet.Range("G27").Interior.Color = LIGHT_GRAY

    worksheet.Columns("H").ColumnWidth = 20
    worksheet.Columns("I").ColumnWidth = 17
    worksheet.Columns("J").ColumnWidth = 19
    worksheet.Columns("K").ColumnWidth = 2
    worksheet.Rows(2).RowHeight = 23
    worksheet.Range("A1:J27").Font.Name = "Calibri"
    worksheet.Application.ActiveWindow.SplitRow = 4
    worksheet.Application.ActiveWindow.FreezePanes = True
    worksheet.Application.ActiveWindow.Zoom = 90
    try:
        worksheet.PageSetup.PrintArea = "$A$1:$J$27"
        worksheet.PageSetup.Orientation = XL_LANDSCAPE
        worksheet.PageSetup.Zoom = False
        worksheet.PageSetup.FitToPagesWide = 1
        worksheet.PageSetup.FitToPagesTall = 1
    except pywintypes.com_error:
        pass

    enterprise_cores = counts[5] * 2 + counts[8] * 2 + counts[10] * 2 + counts[11]
    standard_cores = counts[6] * 2 + counts[7] * 2 + counts[9] * 2 + counts[12]
    current_license_annual = (
        counts[5] * ENTERPRISE_ANNUAL
        + counts[6] * STANDARD_ANNUAL
        + counts[7] * (STANDARD_PERPETUAL + STANDARD_ANNUAL)
        + counts[8] * (ENTERPRISE_PERPETUAL + ENTERPRISE_ANNUAL)
        + counts[9] * STANDARD_ANNUAL
        + counts[10] * ENTERPRISE_ANNUAL
    ) * (1.0 - ea_discount)
    existing_paygo_gross = (
        counts[11] * ENTERPRISE_PAYGO + counts[12] * STANDARD_PAYGO
    ) * existing_paygo_hours * 12
    switch_paygo_gross = (
        enterprise_cores * ENTERPRISE_PAYGO + standard_cores * STANDARD_PAYGO
    ) * switch_hours * 12
    denominator = switch_paygo_gross - existing_paygo_gross
    break_even_discount = (
        1.0 - current_license_annual / denominator
        if not math.isclose(denominator, 0.0)
        else 0.0
    )
    current_annual = current_license_annual + existing_paygo_gross * (1.0 - paygo_discount)
    paygo_annual = switch_paygo_gross * (1.0 - paygo_discount)
    selected_difference = paygo_annual - current_annual
    break_even_difference = switch_paygo_gross * (1.0 - break_even_discount) - (
        current_license_annual + existing_paygo_gross * (1.0 - break_even_discount)
    )
    if abs(break_even_difference) >= 0.01:
        raise RuntimeError(
            f"Independent break-even calculation did not reach zero: {break_even_difference:.6f}"
        )

    return {
        "ea_discount": ea_discount,
        "paygo_discount": paygo_discount,
        "switch_hours": switch_hours,
        "current_annual": current_annual,
        "paygo_annual": paygo_annual,
        "selected_difference": selected_difference,
        "break_even_discount": break_even_discount,
        "break_even_difference": break_even_difference,
    }


def validate_and_save(excel, workbook, business_case, price_sheet, summary) -> dict[str, float]:
    excel.CalculateFullRebuild()

    enterprise_rate = numeric(business_case.Range("C20").Value2, math.nan)
    standard_rate = numeric(business_case.Range("C21").Value2, math.nan)
    if not math.isclose(enterprise_rate, ENTERPRISE_PAYGO, abs_tol=0.000001):
        raise RuntimeError(f"Enterprise PAYGO rate did not resolve: {enterprise_rate}")
    if not math.isclose(standard_rate, STANDARD_PAYGO, abs_tol=0.000001):
        raise RuntimeError(f"Standard PAYGO rate did not resolve: {standard_rate}")

    current_annual = numeric(business_case.Range("G15").Value2, math.nan)
    selected_paygo_annual = numeric(business_case.Range("G24").Value2, math.nan)
    selected_difference = numeric(business_case.Range("G25").Value2, math.nan)
    break_even_discount = numeric(business_case.Range("J8").Value2, math.nan)
    if not math.isclose(current_annual, summary["current_annual"], abs_tol=0.05):
        raise RuntimeError(
            f"Excel current annual cost {current_annual} differs from independent calculation {summary['current_annual']}"
        )
    if not math.isclose(
        break_even_discount,
        summary["break_even_discount"],
        abs_tol=0.0000001,
    ):
        raise RuntimeError(
            f"Excel break-even discount {break_even_discount} differs from independent calculation {summary['break_even_discount']}"
        )

    selected_discount = numeric(business_case.Range("J5").Value2)
    business_case.Range("J5").Value2 = break_even_discount
    excel.CalculateFullRebuild()
    tested_difference = numeric(business_case.Range("G25").Value2, math.nan)
    if abs(tested_difference) >= 1.0:
        raise RuntimeError(
            f"Excel break-even test did not reach within $1 of zero: {tested_difference:.6f}"
        )
    business_case.Range("J5").Value2 = selected_discount
    excel.CalculateFullRebuild()

    key_formulas = {
        "E9": "=SQL_SE_Annual_2Core*(1-EA_SQL_Discount)",
        "E10": "=SQL_EE_Annual_2Core*(1-EA_SQL_Discount)",
        "C20": "=SQL_EE_PAYG_CoreHour",
        "C21": "=SQL_SE_PAYG_CoreHour",
        "G20": "=F20*(1-Azure_PAYGO_Discount)",
        "G21": "=F21*(1-Azure_PAYGO_Discount)",
        "G25": "=G24-G15",
        "J9": "=$G$25",
    }
    for cell_address, expected_formula in key_formulas.items():
        actual_formula = str(business_case.Range(cell_address).Formula)
        if actual_formula.upper() != expected_formula.upper():
            raise RuntimeError(
                f"Unexpected formula in {cell_address}: {actual_formula}"
            )

    price_checks = {
        "D5": ENTERPRISE_PERPETUAL,
        "D6": STANDARD_PERPETUAL,
        "D13": ENTERPRISE_ANNUAL,
        "D14": STANDARD_ANNUAL,
        "D17": ENTERPRISE_PAYGO,
        "D18": STANDARD_PAYGO,
    }
    for cell_address, expected_value in price_checks.items():
        actual_value = numeric(price_sheet.Range(cell_address).Value2, math.nan)
        if not math.isclose(actual_value, expected_value, abs_tol=0.000001):
            raise RuntimeError(
                f"Unexpected source price in {cell_address}: {actual_value}"
            )

    business_case.Activate()
    business_case.Range("J5").Select()
    workbook.Windows.Item(1).Visible = True
    workbook.CheckCompatibility = False
    workbook.Save()

    return {
        "current_annual": current_annual,
        "paygo_annual": selected_paygo_annual,
        "selected_difference": selected_difference,
        "break_even_discount": break_even_discount,
        "tested_difference": tested_difference,
    }


def verify_saved_workbook(excel) -> None:
    workbook = None
    try:
        workbook = excel.Workbooks.Open(
            str(WORKBOOK_PATH),
            UpdateLinks=0,
            ReadOnly=True,
            IgnoreReadOnlyRecommended=True,
        )
        sheet_names = [worksheet.Name for worksheet in workbook.Worksheets]
        if sheet_names[:2] != ["Business Case", "SQL License Book Prices"]:
            raise RuntimeError(f"Unexpected saved sheet order: {sheet_names}")
        if not workbook.Windows.Item(1).Visible:
            raise RuntimeError("Workbook saved with its document window hidden.")
        business_case = workbook.Worksheets.Item("Business Case")
        price_sheet = workbook.Worksheets.Item("SQL License Book Prices")
        if not math.isclose(
            numeric(business_case.Range("C20").Value2, math.nan),
            ENTERPRISE_PAYGO,
            abs_tol=0.000001,
        ):
            raise RuntimeError("Saved Enterprise PAYGO rate did not persist.")
        if not math.isclose(
            numeric(price_sheet.Range("D5").Value2, math.nan),
            ENTERPRISE_PERPETUAL,
            abs_tol=0.000001,
        ):
            raise RuntimeError("Saved SQL book price sheet did not persist.")
    finally:
        if workbook is not None:
            workbook.Close(False)


def main() -> None:
    if not WORKBOOK_PATH.exists():
        raise FileNotFoundError(f"Workbook not found: {WORKBOOK_PATH}")

    excel = win32.DispatchEx("Excel.Application")
    excel.Visible = False
    excel.DisplayAlerts = False
    excel.ScreenUpdating = False
    excel.AskToUpdateLinks = False
    workbook = None
    saved = False
    summary = None
    validation = None
    try:
        workbook = excel.Workbooks.Open(
            str(WORKBOOK_PATH),
            UpdateLinks=0,
            ReadOnly=False,
            IgnoreReadOnlyRecommended=True,
        )
        excel.Calculation = XL_AUTOMATIC
        business_case = workbook.Worksheets.Item("Business Case")
        price_sheet = build_price_sheet(workbook, business_case)
        summary = update_business_case(workbook, business_case)
        validation = validate_and_save(
            excel,
            workbook,
            business_case,
            price_sheet,
            summary,
        )
        saved = True
        workbook.Close(False)
        workbook = None
        verify_saved_workbook(excel)
    finally:
        if workbook is not None:
            workbook.Close(False)
        excel.Quit()

    if not saved or summary is None or validation is None:
        raise RuntimeError("Workbook update did not complete.")

    print(f"Workbook: {WORKBOOK_PATH}")
    print(f"EA SQL discount: {summary['ea_discount']:.4%}")
    print(f"Selected Azure PAYGO discount: {summary['paygo_discount']:.2%}")
    print(f"Switch PAYGO hours/month: {summary['switch_hours']:.0f}")
    print(f"Current annual cost: ${validation['current_annual']:,.2f}")
    print(f"PAYGO annual cost at selected discount: ${validation['paygo_annual']:,.2f}")
    print(f"Difference at selected discount: ${validation['selected_difference']:,.2f}")
    print(f"Break-even PAYGO discount: {validation['break_even_discount']:.4%}")
    print(f"Excel break-even test difference: ${validation['tested_difference']:,.6f}")


if __name__ == "__main__":
    main()